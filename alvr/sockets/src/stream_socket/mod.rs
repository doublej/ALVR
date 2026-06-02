// Note: for StreamSocket, the client uses a server socket, the server uses a client socket.
// This is because of certificate management. The server needs to trust a client and its certificate
//
// StreamSender and StreamReceiver endpoints allow for convenient conversion of the header to/from
// bytes while still handling the additional byte buffer with zero copies and extra allocations.

// Performance analysis:
// We want to minimize the transmission time for various sizes of packets.
// The current code locks the write socket *per shard* and not *per packet*. This leds to the best
// performance outcome given that the possible packets can be either very small (one shard) or very
// large (hundreds/thousands of shards, for video). if we don't allow interleaving shards, a very
// small packet will need to wait a long time before getting received if there was an ongoing
// transmission of a big packet before. If we allow interleaving shards, small packets can be
// transmitted quicker, with only minimal latency increase for the ongoing transmission of the big
// packet.
// Note: We can't clone the underlying socket for each StreamSender and the mutex around the socket
// cannot be removed. This is because we need to make sure at least shards are written whole.

mod tcp;
mod udp;

use alvr_common::{
    AnyhowToCon, ConResult, HandleTryAgain, ToCon, anyhow::Result, parking_lot::Mutex,
};
use alvr_session::{DscpTos, SocketBufferConfig, SocketProtocol};
use bincode::config;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    cmp::Ordering,
    collections::HashMap,
    marker::PhantomData,
    mem,
    net::{IpAddr, TcpListener, UdpSocket},
    ops::{Deref, DerefMut},
    sync::{Arc, mpsc},
    time::Duration,
};

trait MultiplexedSocketWriter {
    // Note: consts are not trait-safe, we require a method
    fn payload_offset(&self) -> usize;

    // Max payload bytes per shard for transports that support incremental (sliced) sending, or None
    // for transports that must send each packet whole (e.g. TCP).
    fn incremental_shard_size(&self) -> Option<usize> {
        None
    }

    fn send(&mut self, stream_id: u16, packet_index: u32, buffer: &mut Vec<u8>) -> Result<()>;

    // Send a single already-laid-out shard of an incremental packet. `shard` starts with
    // `payload_offset()` bytes of prefix space (overwriting the tail of the previously sent shard,
    // which has already left the socket). Only called when `incremental_shard_size()` is Some.
    fn send_shard(
        &mut self,
        _stream_id: u16,
        _packet_index: u32,
        _capacity_hint: u32,
        _shard_index: u32,
        _is_final: bool,
        _shard: &mut [u8],
    ) -> Result<()> {
        unreachable!("send_shard called on a transport without incremental support")
    }
}

struct ReconstructedPacket {
    index: u32,
    buffer: Vec<u8>,
}

struct StreamRecvQueues {
    used_buffer_sender: mpsc::Sender<Vec<u8>>,
    used_buffer_receiver: mpsc::Receiver<Vec<u8>>,
    packet_queue: mpsc::Sender<ReconstructedPacket>,
}

trait MultiplexedSocketReader {
    fn payload_offset(&self) -> usize;

    fn recv(&mut self, stream_queues: &HashMap<u16, StreamRecvQueues>) -> ConResult;
}

/// Memory buffer that contains a hidden prefix
#[derive(Default)]
pub struct Buffer<H = ()> {
    inner: Vec<u8>,
    raw_payload_offset: usize, // this corresponds to prefix + header
    _phantom: PhantomData<H>,
}

impl<H> Deref for Buffer<H> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.inner[self.raw_payload_offset..]
    }
}

impl<H> DerefMut for Buffer<H> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner[self.raw_payload_offset..]
    }
}

#[derive(Clone)]
pub struct StreamSender<H> {
    inner: Arc<Mutex<Box<dyn MultiplexedSocketWriter + Send>>>,
    stream_id: u16,
    payload_offset: usize,
    next_packet_index: u32,
    used_buffers: Vec<Vec<u8>>,
    _phantom: PhantomData<H>,
}

impl<H> StreamSender<H> {
    /// Shard and send a buffer with zero copies and zero allocations.
    /// The prefix of each shard is written over the previously sent shard to avoid reallocations.
    pub fn send(&mut self, mut buffer: Buffer<H>) -> Result<()> {
        self.inner
            .lock()
            .send(self.stream_id, self.next_packet_index, &mut buffer.inner)?;

        self.used_buffers.push(buffer.inner);

        self.next_packet_index = self.next_packet_index.wrapping_add(1);

        Ok(())
    }
}

impl<H: Serialize> StreamSender<H> {
    pub fn get_buffer(&mut self, header: &H, raw_payload_len: usize) -> Result<Buffer<H>> {
        let mut buffer = self.used_buffers.pop().unwrap_or_default();

        buffer.resize(self.payload_offset, 0);

        let encoded_payload_size =
            bincode::serde::encode_into_std_write(header, &mut buffer, config::standard())?;

        let raw_payload_offset = self.payload_offset + encoded_payload_size;

        buffer.resize(raw_payload_offset + raw_payload_len, 0);

        Ok(Buffer {
            inner: buffer,
            raw_payload_offset,
            _phantom: PhantomData,
        })
    }

    pub fn send_header_with_payload(&mut self, header: &H, raw_payload: &[u8]) -> Result<()> {
        let mut buffer = self.get_buffer(header, raw_payload.len())?;
        buffer.copy_from_slice(raw_payload);
        self.send(buffer)
    }

    pub fn send_header(&mut self, header: &H) -> Result<()> {
        self.send_header_with_payload(header, &[])
    }

    /// Begin an incremental ("sliced") packet whose payload is produced over time. Completed shards
    /// are flushed to the socket as soon as enough payload is pushed, overlapping transmission with
    /// the production of later slices. This is what lets video encode and network transmission run
    /// concurrently instead of strictly one-after-the-other.
    ///
    /// `max_payload_size` is an upper bound on the payload bytes that will be pushed (excluding the
    /// header). It must not be exceeded: the receiver uses it to size its reassembly buffer, and
    /// shards beyond the bound would be rejected. On transports without incremental support (TCP)
    /// the payload is buffered and sent whole on `finish()`.
    pub fn begin_frame(&mut self, header: &H, max_payload_size: usize) -> Result<FrameSender<'_, H>> {
        let packet_index = self.next_packet_index;
        self.next_packet_index = self.next_packet_index.wrapping_add(1);

        let shard_data_size = self.inner.lock().incremental_shard_size();

        let mut buffer = self.used_buffers.pop().unwrap_or_default();
        buffer.clear();
        buffer.resize(self.payload_offset, 0);
        let header_size =
            bincode::serde::encode_into_std_write(header, &mut buffer, config::standard())?;

        // Upper bound on the total shard count (header + payload), plus one shard of slack to absorb
        // off-by-one from the header. Used as the receiver's reservation hint.
        let capacity_hint = shard_data_size
            .map(|s| (header_size + max_payload_size).div_ceil(s) as u32 + 1)
            .unwrap_or(0);

        Ok(FrameSender {
            sender: self,
            buffer,
            shard_data_size,
            capacity_hint,
            packet_index,
            sent_shards: 0,
        })
    }
}

/// In-progress incremental packet. Created by [`StreamSender::begin_frame`]. Call [`Self::push`] as
/// payload becomes available and [`Self::finish`] when the packet is complete.
pub struct FrameSender<'a, H> {
    sender: &'a mut StreamSender<H>,
    buffer: Vec<u8>, // [prefix space][header][payload...]
    shard_data_size: Option<usize>,
    capacity_hint: u32,
    packet_index: u32,
    sent_shards: usize,
}

impl<H> FrameSender<'_, H> {
    /// Append a slice of payload, flushing any shards that are now fully available. Only shards that
    /// are guaranteed not to be the final one are flushed here; the tail is sent by `finish()`.
    pub fn push(&mut self, data: &[u8]) -> Result<()> {
        self.buffer.extend_from_slice(data);

        let Some(shard_data_size) = self.shard_data_size else {
            return Ok(()); // non-incremental transport: buffer everything, send on finish
        };
        let prefix = self.sender.payload_offset;

        // Flush full shards with strictly more data after them, so they are never the final shard.
        while self.buffer.len() > (self.sent_shards + 1) * shard_data_size + prefix {
            self.flush_shard(shard_data_size, self.sent_shards, false)?;
            self.sent_shards += 1;
        }

        Ok(())
    }

    /// Flush the remaining payload (marking the last shard final) and recycle the buffer.
    pub fn finish(mut self) -> Result<()> {
        let prefix = self.sender.payload_offset;

        if let Some(shard_data_size) = self.shard_data_size {
            let payload_size = self.buffer.len() - prefix;
            let total_shards = payload_size.div_ceil(shard_data_size).max(1);

            while self.sent_shards < total_shards {
                let is_final = self.sent_shards + 1 == total_shards;
                self.flush_shard(shard_data_size, self.sent_shards, is_final)?;
                self.sent_shards += 1;
            }
        } else {
            // No incremental support: send the whole packet in one shot.
            self.sender.inner.lock().send(
                self.sender.stream_id,
                self.packet_index,
                &mut self.buffer,
            )?;
        }

        self.sender.used_buffers.push(mem::take(&mut self.buffer));

        Ok(())
    }

    fn flush_shard(
        &mut self,
        shard_data_size: usize,
        shard_index: usize,
        is_final: bool,
    ) -> Result<()> {
        let prefix = self.sender.payload_offset;
        let payload_size = self.buffer.len() - prefix;
        let shard_start = shard_index * shard_data_size;
        let shard_size = usize::min(shard_data_size, payload_size - shard_start);

        // The shard view starts at `shard_start` (not `prefix + shard_start`): its first `prefix`
        // bytes overwrite the tail of the previous, already-sent shard, matching the receiver layout.
        let shard = &mut self.buffer[shard_start..][..prefix + shard_size];

        self.sender.inner.lock().send_shard(
            self.sender.stream_id,
            self.packet_index,
            self.capacity_hint,
            shard_index as u32,
            is_final,
            shard,
        )
    }
}

pub struct ReceiverData<H> {
    buffer: Vec<u8>,
    payload_offset: usize,
    used_buffer_queue: mpsc::Sender<Vec<u8>>,
    had_packet_loss: bool,
    _phantom: PhantomData<H>,
}

impl<H> ReceiverData<H> {
    pub fn had_packet_loss(&self) -> bool {
        self.had_packet_loss
    }
}

impl<H: DeserializeOwned> ReceiverData<H> {
    pub fn get(&self) -> Result<(H, &[u8])> {
        let payload = &self.buffer[self.payload_offset..];

        let (header, decoded_size) =
            bincode::serde::decode_from_slice(payload, config::standard())?;

        Ok((header, &payload[decoded_size..]))
    }

    pub fn get_header(&self) -> Result<H> {
        Ok(self.get()?.0)
    }
}

impl<H> Drop for ReceiverData<H> {
    fn drop(&mut self) {
        self.used_buffer_queue
            .send(mem::take(&mut self.buffer))
            .ok();
    }
}

pub struct StreamReceiver<H> {
    payload_offset: usize,
    packet_receiver: mpsc::Receiver<ReconstructedPacket>,
    used_buffer_queue: mpsc::Sender<Vec<u8>>,
    last_packet_index: Option<u32>,
    _phantom: PhantomData<H>,
}

// Wrapping comparison for packet indices.
// Problem: packet indices have a finite bit-width and we have to wrap around upon reaching the
// maximum value. This function provides proper ordering capability when wrapping around. The
// maximum index distance that two packets can have is u32::MAX / 2 (which is plenty for any
// reasonable circumstance).
fn wrapping_cmp(lhs: u32, rhs: u32) -> Ordering {
    const LOWER_BOUND: u32 = u32::MAX / 4;
    const UPPER_BOUND: u32 = u32::MAX - LOWER_BOUND;

    if lhs < LOWER_BOUND && rhs > UPPER_BOUND {
        // lhs wrapped around, so its "unwrapped" value would be `lhs as u64 + u32::MAX`
        Ordering::Greater
    } else if lhs > UPPER_BOUND && rhs < LOWER_BOUND {
        // rhs wrapped around
        Ordering::Less
    } else {
        lhs.cmp(&rhs)
    }
}

impl<H: DeserializeOwned + Serialize> StreamReceiver<H> {
    pub fn recv(&mut self, timeout: Duration) -> ConResult<ReceiverData<H>> {
        let packet = self
            .packet_receiver
            .recv_timeout(timeout)
            .handle_try_again()?;

        let mut had_packet_loss = false;

        if let Some(last_idx) = self.last_packet_index {
            // Use wrapping arithmetics
            match wrapping_cmp(packet.index, last_idx.wrapping_add(1)) {
                Ordering::Equal => (),
                Ordering::Greater => {
                    // Skipped some indices
                    had_packet_loss = true
                }
                Ordering::Less => {
                    // Old packet, discard
                    self.used_buffer_queue.send(packet.buffer).to_con()?;
                    return alvr_common::try_again();
                }
            }
        }
        self.last_packet_index = Some(packet.index);

        Ok(ReceiverData {
            buffer: packet.buffer,
            payload_offset: self.payload_offset,
            used_buffer_queue: self.used_buffer_queue.clone(),
            had_packet_loss,
            _phantom: PhantomData,
        })
    }
}

pub enum StreamSocketBuilder {
    Tcp(TcpListener),
    Udp(UdpSocket),
}

impl StreamSocketBuilder {
    pub fn listen_for_server(
        timeout: Duration,
        port: u16,
        stream_socket_config: SocketProtocol,
        stream_tos_config: Option<DscpTos>,
        buffer_config: SocketBufferConfig,
    ) -> Result<Self> {
        Ok(match stream_socket_config {
            SocketProtocol::Udp => {
                StreamSocketBuilder::Udp(udp::bind(port, stream_tos_config, buffer_config)?)
            }
            SocketProtocol::Tcp => StreamSocketBuilder::Tcp(tcp::bind(
                timeout,
                port,
                stream_tos_config,
                buffer_config,
            )?),
        })
    }

    pub fn accept_from_server(
        self,
        server_ip: IpAddr,
        port: u16,
        max_packet_size: usize,
        timeout: Duration,
    ) -> ConResult<StreamSocket> {
        let (send_socket, receive_socket) = match self {
            StreamSocketBuilder::Udp(socket) => {
                udp::connect(&socket, server_ip, port, timeout).to_con()?;
                udp::split_multiplexed(socket, max_packet_size).to_con()?
            }
            StreamSocketBuilder::Tcp(listener) => {
                let socket = tcp::accept_from_server(&listener, Some(server_ip), timeout)?;
                tcp::split_multiplexed(socket, timeout).to_con()?
            }
        };

        Ok(StreamSocket {
            send_socket: Arc::new(Mutex::new(send_socket)),
            receive_socket,
            queues: HashMap::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn connect_to_client(
        timeout: Duration,
        client_ip: IpAddr,
        port: u16,
        protocol: SocketProtocol,
        dscp: Option<DscpTos>,
        buffer_config: SocketBufferConfig,
        max_packet_size: usize,
    ) -> ConResult<StreamSocket> {
        let (send_socket, receive_socket) = match protocol {
            SocketProtocol::Udp => {
                let socket = udp::bind(port, dscp, buffer_config).to_con()?;
                udp::connect(&socket, client_ip, port, timeout).to_con()?;
                udp::split_multiplexed(socket, max_packet_size).to_con()?
            }
            SocketProtocol::Tcp => {
                let socket = tcp::connect_to_client(timeout, &[client_ip], port, buffer_config)?;
                tcp::split_multiplexed(socket, timeout).to_con()?
            }
        };

        Ok(StreamSocket {
            send_socket: Arc::new(Mutex::new(send_socket)),
            receive_socket,
            queues: HashMap::new(),
        })
    }
}

pub struct StreamSocket {
    send_socket: Arc<Mutex<Box<dyn MultiplexedSocketWriter + Send>>>,
    receive_socket: Box<dyn MultiplexedSocketReader + Send>,
    queues: HashMap<u16, StreamRecvQueues>,
}

impl StreamSocket {
    pub fn request_stream<T>(&self, stream_id: u16) -> StreamSender<T> {
        StreamSender {
            inner: Arc::clone(&self.send_socket),
            stream_id,
            payload_offset: self.send_socket.lock().payload_offset(),
            next_packet_index: 0,
            used_buffers: vec![],
            _phantom: PhantomData,
        }
    }

    // max_concurrent_buffers: number of buffers allocated by this call which will be reused to
    // receive packets for this stream ID. If packets are not read fast enough, the shards received
    // for this particular stream will be discarded
    pub fn subscribe_to_stream<T>(
        &mut self,
        stream_id: u16,
        max_concurrent_buffers: usize,
    ) -> StreamReceiver<T> {
        let (packet_sender, packet_receiver) = mpsc::channel();
        let (used_buffer_sender, used_buffer_receiver) = mpsc::channel();

        for _ in 0..max_concurrent_buffers {
            used_buffer_sender.send(vec![]).ok();
        }

        self.queues.insert(
            stream_id,
            StreamRecvQueues {
                used_buffer_sender: used_buffer_sender.clone(),
                used_buffer_receiver,
                packet_queue: packet_sender,
            },
        );

        StreamReceiver {
            payload_offset: self.receive_socket.payload_offset(),
            packet_receiver,
            used_buffer_queue: used_buffer_sender,
            last_packet_index: None,
            _phantom: PhantomData,
        }
    }

    pub fn recv(&mut self) -> ConResult {
        self.receive_socket.recv(&self.queues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::{Ipv4Addr, UdpSocket},
        sync::atomic::{AtomicBool, Ordering},
        thread,
        time::Instant,
    };

    const MAX_PACKET_SIZE: usize = 1400;
    const STREAM_ID: u16 = 7;

    // Build a connected pair of loopback StreamSockets (a -> sends, b -> receives).
    fn loopback_pair() -> (StreamSocket, StreamSocket) {
        let a = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let b = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let (addr_a, addr_b) = (a.local_addr().unwrap(), b.local_addr().unwrap());
        a.connect(addr_b).unwrap();
        b.connect(addr_a).unwrap();
        a.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        b.set_read_timeout(Some(Duration::from_millis(50))).unwrap();

        let (writer_a, reader_a) = udp::split_multiplexed(a, MAX_PACKET_SIZE).unwrap();
        let (writer_b, reader_b) = udp::split_multiplexed(b, MAX_PACKET_SIZE).unwrap();

        let sender = StreamSocket {
            send_socket: Arc::new(Mutex::new(writer_a)),
            receive_socket: reader_a,
            queues: HashMap::new(),
        };
        let receiver = StreamSocket {
            send_socket: Arc::new(Mutex::new(writer_b)),
            receive_socket: reader_b,
            queues: HashMap::new(),
        };

        (sender, receiver)
    }

    fn recv_with_retry<H: DeserializeOwned + Serialize>(
        receiver: &mut StreamReceiver<H>,
    ) -> ReceiverData<H> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match receiver.recv(Duration::from_millis(50)) {
                Ok(data) => return data,
                Err(_) if Instant::now() < deadline => continue,
                Err(_) => panic!("timed out waiting for packet"),
            }
        }
    }

    // A whole-buffer send and an incremental (sliced) send of the same payload must reassemble to
    // byte-identical data on the receiver. This guards both the unchanged fast path and the new
    // FrameSender path, including the prefix/is_final protocol change.
    #[test]
    fn whole_and_incremental_reassemble_identically() {
        let (sender_socket, mut receiver_socket) = loopback_pair();

        let mut sender = sender_socket.request_stream::<u64>(STREAM_ID);
        let mut receiver = receiver_socket.subscribe_to_stream::<u64>(STREAM_ID, 8);

        let stop = Arc::new(AtomicBool::new(false));
        let pump = {
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    receiver_socket.recv().ok();
                }
            })
        };

        // Spans several shards (1400 MTU - 14 prefix = 1386 payload bytes per shard).
        let payload: Vec<u8> = (0..9000u32).map(|i| i.wrapping_mul(2654435761) as u8).collect();
        let header = 0xDEAD_BEEFu64;

        // 1) whole-buffer
        sender.send_header_with_payload(&header, &payload).unwrap();
        let got = recv_with_retry(&mut receiver);
        let (h, data) = got.get().unwrap();
        assert_eq!(h, header);
        assert_eq!(data, payload.as_slice(), "whole-buffer payload mismatch");

        // 2) incremental, pushed in awkwardly-sized chunks that don't align to shard boundaries
        let mut frame = sender.begin_frame(&header, payload.len()).unwrap();
        for chunk in payload.chunks(577) {
            frame.push(chunk).unwrap();
        }
        frame.finish().unwrap();
        let got = recv_with_retry(&mut receiver);
        let (h, data) = got.get().unwrap();
        assert_eq!(h, header);
        assert_eq!(data, payload.as_slice(), "incremental payload mismatch");

        stop.store(true, Ordering::Relaxed);
        pump.join().unwrap();
    }
}
