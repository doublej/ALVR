# Sliced (subframe-readback) video streaming

**Goal:** cut motion-to-photon latency by overlapping NVENC **encode** with **network
transmission** — transmit each slice of a frame the moment the encoder finishes it, instead of
waiting for the whole frame. The client still reassembles the **whole frame** before decoding
(no partial decode), which sidesteps the net-negative client-side partial-decode result from
upstream issue #2436. This is the technique VirtualDesktop uses to feel snappier than stock ALVR.

Target rig: RTX 4080 Super (Ada, dual NVENC), Windows, Quest 3, wired.

## Status

| Layer | Crate / file | State |
|-------|--------------|-------|
| Transport (shard protocol + reassembly) | `alvr/sockets/.../udp.rs`, `mod.rs` | Done. Loopback test proves whole-buffer and incremental sends reassemble **byte-identical**. Builds on macOS (nightly) + Windows (stable). |
| server_core send path (sl2a) | `server_core/src/{connection,lib,c_api}.rs` | Done. Compiles on macOS + Windows. |
| server_openvr FFI (sl2b) | `server_openvr/src/lib.rs`, `cpp/.../bindings.h`, `alvr_server.cpp` | Done. `server_openvr` builds+links on Windows. |
| NVENC subframe readback (sl3) | `cpp/platform/win32/NvEncoder.{cpp,h}`, `VideoEncoderNVENC.{cpp,h}`, `NalParsing.cpp` | Done. `server_openvr` builds+links on Windows. **Runtime behavior not yet validated on hardware.** |

All committed locally on branch `diagnostics` (not pushed).

### Adversarial review pass

A multi-agent adversarial review (5 dimensions → independent verification) found and fixed **9 real
issues**, including one **critical** crash:

| # | Sev | Fix |
|---|-----|-----|
| 1 | critical | NVENC poll wrapped `nvEncLockBitstream(doNotWait=1)` in the throwing `NVENC_API_CALL`; `NV_ENC_ERR_LOCK_BUSY` ("no slice ready yet") would have crashed the encode thread on the first sliced frame. Now treated as yield+retry. |
| 2 | high | `STREAM_CORRUPTED` was re-checked per slice → a frame could be dropped mid-stream, stranding a partial `FrameSender`. Drop decision now latched once per frame (`DROPPING_FRAME`). |
| 3-5,9 | high/med | `maxFrameSize` was `width*height` (not a true upper bound); the receiver hard-rejects shards past the implied capacity, so an oversized frame would never reassemble. Now `width*height*3/2` (raw NV12 ceiling). |
| 6 | low | Wire-supplied `capacity_hint` drove an unbounded `reserve()` (multi-TB DoS). Clamped to `MAX_CAPACITY_HINT`. |
| 7 | low | Orphan `is_last` (first slice dropped upstream) reported stats for a never-sent frame. Whole frame now dropped consistently (`SLICE_FRAME_DROPPED`). |
| 8 | low | `send_video_slice` built a slice from a possibly-null pointer (UB) on degenerate frames. Now null/zero-length safe. |

After the fixes: sockets reassembly test green (macOS), `server_core` compiles, and the full
streamer (driver + dashboard) rebuilds clean on Windows.

## Architecture

```
NVENC encodes frame in N slices (sliceMode=3, enableSubFrameWrite=1)
  -> EncodeFrameSliced polls nvEncLockBitstream(doNotWait=1); each new chunk:
       -> ParseFrameNalsSliced (strips SPS/PPS/VPS on first slice)
         -> VideoSliceSend FFI
           -> ServerCoreContext::send_video_slice  (VideoStreamPacket::Slice)
             -> video_send_thread: FrameSender::{begin_frame|push|finish}
               -> UDP shards on the wire, sent as each slice finishes
                 -> client reassembles ALL shards into the whole frame
                   -> MediaCodec decodes the complete frame (unchanged)
```

### Wire protocol change (`udp.rs`)

The 14-byte shard prefix is unchanged in size but two fields were reinterpreted:

```
[stream_id u16][packet_index u32][capacity_hint u32][is_final(1) | shard_index(31)]
```

- `capacity_hint` (was `shards_count`): an **upper bound** on the shard count, used by the receiver
  to pre-reserve buffer capacity. Over-estimates are harmless; the real end is the `is_final` flag.
- High bit of the last field is `IS_FINAL_BIT` (`1<<31`); `SHARD_INDEX_MASK = !IS_FINAL_BIT`.
- A packet is complete when the `is_final` shard has arrived **and**
  `received_shard_indices.len() == final_shard_index + 1`.

Both server and client build from this same code, so the format is internally consistent. The
whole-buffer `send()` path sets `capacity_hint = shards_count` and `is_final` on the last shard, so
it is behaviour-identical to before.

## Enabling it (live test)

Sliced encoding is **off by default** and gated by an environment variable read at encoder init:

```
ALVR_SLICED_ENCODING = <number of slices per frame>   (2..32; unset/<=1 = off)
```

- Only H264/HEVC (AV1 falls back to whole-frame).
- Recommended first value: `4`.
- When enabled, the SteamVR driver log prints: `NVENC sliced encoding enabled: N slices/frame`.

Set it for the SteamVR/driver process. Easiest: set it as a user/system env var, or in the shell
that launches SteamVR, before starting a session. Then run a normal ALVR session.

## Build & run on the Windows rig (Fractal)

Deps are already prepared (`deps/windows`: ffmpeg, x264, libvpl built). To rebuild after edits:

```powershell
cd C:\Users\jurre\alvr
cargo xtask build-streamer            # debug; add --release for realistic latency
# or, to iterate on just the driver:
cargo build -p alvr_server_openvr
```

## What to measure

Use the on-HUD encode/network timing (optimization n20 in OPTIMIZATION_PLAN.md) or the dashboard
statistics. The win shows up as a reduction in **(encode start) -> (last byte on wire)** and in
total **motion-to-photon**, *not* in raw encode time. Compare `ALVR_SLICED_ENCODING` unset vs `4`
(and try `2`, `8`) at a fixed bitrate/resolution. Watch for:

- Lower transmit-start latency / smoother frame pacing.
- No increase in decode errors or "Waiting for IDR" spam (would indicate reassembly trouble).
- CPU: the readback loop polls; if a core spikes, raise the yield/sleep in `EncodeFrameSliced`.

## Risk points the live test validates (not provable at compile time)

1. **`hwEncodeStatus == 2` == frame complete.** Confirmed by NVIDIA devs on the forum, used as the
   loop's termination signal, but the exact value is not documented in the header. There is a
   `MAX_POLLS` safety bound. If frames hang or truncate, log `hwEncodeStatus`/`bitstreamSizeInBytes`
   per poll and adjust.
2. **Polling `doNotWait=1` while `enableEncodeAsync=1`.** We left async on (only `enableSubFrameWrite`
   was added) on the theory that subframe write makes partial data lockable regardless of async.
   `NV_ENC_ERR_LOCK_BUSY` ("no slice ready yet") is now handled as a retry, so polling is safe; but
   if partial reads never actually appear (the loop only ever sees the whole frame at once), the
   overlap won't materialize — try forcing `enableEncodeAsync=0` for the sliced path.
3. **`NV_ENC_CAPS_SUPPORT_SUBFRAME_READBACK`** is assumed supported on the 4080S. Add a capability
   check + fallback if any GPU reports unsupported.
4. **sliceMode=3 / N slices** interaction with intra-refresh and the chosen preset/tuning.

## Next steps after validation

- Promote `ALVR_SLICED_ENCODING` from an env var to a real dashboard setting (Rust schema in
  `session/src/settings.rs` -> flattened in `session/src/lib.rs` -> `connection.rs` -> C++ `Settings`).
- Add the `NV_ENC_CAPS_SUPPORT_SUBFRAME_READBACK` capability guard.
- Consider tuning N per resolution (MB-row alignment).

> Note: NVENC **Split-Frame Encoding** (multi-engine, the 4080S has 2 NVENC engines) is a *separate*
> optimization and is **incompatible with subframe readback** per the NVENC programming guide — pick
> one. Sliced streaming (this doc) targets latency; SFE targets encoder throughput.
