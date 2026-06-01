# ALVR Client Connection Technical Analysis
## Test Session: iteration_1_20251128_092023

## Executive Summary

The ALVR client (PID 4172) successfully connected to the server **twice** during this test session, but both connections were preceded by connection attempts that failed or required restarts. The analysis reveals a pattern of:
1. Initial socket setup failures
2. Server restart events
3. Successful stream establishment after delays
4. Rapid reconnections following initial success

---

## Timeline of Events

### Application Launch and Initialization
**09:19:36.187** - ALVR client process started (PID: 4172, UID: 10197)
- Device: Oculus Quest (eureka)
- OpenXR runtime initialized
- All required extensions enumerated successfully

### First Connection Attempt

**09:19:37.566** (T+1.38s)
```
Initial socket buffer size: send: 524288B, recv: 1048576B
```
- First socket initialization
- Default kernel socket buffers allocated

**09:19:37.676** (T+1.49s)
```
Server restarting
```
- **CRITICAL**: Server restart detected only 110ms after socket init
- This indicates the server was not ready or encountered an issue during handshake
- Connection aborted before stream could start

**09:19:38.679** (T+2.49s)
```
Initial socket buffer size: send: 524288B, recv: 1048576B
```
- Second socket initialization attempt (1.0s after restart)
- Client attempting to reconnect automatically

### Connection Error Period

**09:20:01.466** (T+25.28s)
```
Connection error: Try again
```
- **ERROR**: Connection failed 22.8 seconds after the second socket init
- Error type: `EAGAIN` / `EWOULDBLOCK` (transient socket error)
- Likely causes:
  - Server handshake timeout
  - Network buffer congestion
  - Server not accepting connections yet

**09:20:02.468** (T+26.28s)
```
Initial socket buffer size: send: 524288B, recv: 1048576B
```
- Third socket initialization (1.0s retry delay after error)
- Client persisting with connection attempts

### First Successful Connection

**09:20:12.623** (T+36.44s)
```
Stream starting
```
- **SUCCESS**: Stream initiation begins
- 10.2 seconds after the last socket init
- This is the first time we see "Stream starting" message

**09:20:12.625** (T+36.44s)
```
Initial socket buffer size: send: 524288B, recv: 1048576B
Set socket send buffer succeeded: 16777216
Set socket recv buffer succeeded: 16777216
```
- Socket buffers upgraded from 512KB/1MB to **16MB/16MB**
- This is the streaming socket configuration (32x increase)
- Large buffers needed for high-bandwidth video/audio data

**09:20:12.626** (T+36.44s)
```
Connected to server
```
- **CONNECTED**: First successful connection established
- Total time from app launch: **36.44 seconds**
- Audio subsystem initialized immediately after

**Connection Sequence Timing:**
- Socket init → Stream starting: **0.002s** (2ms)
- Stream starting → Connected: **0.003s** (3ms)
- **Total handshake duration: 5ms** (extremely fast once ready)

### Post-Connection Activity

**09:20:15.798** (T+39.61s)
```
Initial socket buffer size: send: 524288B, recv: 1048576B
```
- Another socket initialized **3.17 seconds** after connection
- Likely a control socket or secondary channel
- Not a streaming socket (small buffers)

### Second Successful Connection

**09:20:25.769** (T+49.58s)
```
Stream starting
```
- Second stream start event
- 9.97 seconds after the first connection
- 13.1 seconds after previous socket init

**09:20:25.771** (T+49.58s)
```
Initial socket buffer size: send: 524288B, recv: 1048576B
Set socket send buffer succeeded: 16777216
Set socket recv buffer succeeded: 16777216
```
- Socket buffers upgraded to 16MB/16MB again
- Identical configuration to first connection

**09:20:25.772** (T+49.58s)
```
Connected to server
```
- **CONNECTED**: Second successful connection
- **Reconnection timing: 13.1 seconds** after first connection

**Connection Sequence Timing:**
- Socket init → Stream starting: **0.002s** (2ms)
- Stream starting → Connected: **0.003s** (3ms)
- **Identical fast handshake pattern**

---

## Server-Side Handshake Errors (from crash_log.txt)

During the test window (09:19:30 - 09:20:25), the server logged multiple handshake errors:

### Error Timeline

**09:19:30.550**
```
Handshake error for client.wired: Try again
```
- First handshake attempt failed
- 5.36 seconds **before** client process started
- Indicates previous test run or residual connection

**09:19:33.794**
```
Failed to find resumed state line
```
- Server unable to resume previous session
- Session persistence mechanism failed

**09:19:47.065**
```
Handshake error for client.wired: Try again
```
- Second handshake failure
- 10.5 seconds after client launch
- Server still not accepting connections

**09:20:10.376**
```
Handshake error for client.wired: Try again
```
- Third handshake failure
- 34.2 seconds after client launch
- **2.2 seconds before first successful connection**

**09:20:11.970**
```
Handshake error for client.wired: No microphones found
```
- **NEW ERROR TYPE**: Audio device enumeration failure
- Server detected missing microphone during handshake
- This may have been resolved before stream started

**09:20:23.489**
```
Handshake error for client.wired: Try again
```
- Fourth handshake failure
- 10.9 seconds after first successful connection
- **2.3 seconds before second successful connection**

**09:20:25.117**
```
Handshake error for client.wired: No microphones found
```
- Repeated microphone error
- 0.65 seconds before second successful connection
- Connection succeeded **despite this error**

---

## Technical Findings

### 1. Connection Architecture

The ALVR client uses a **multi-socket architecture**:

1. **Control Socket** (small buffers: 512KB send / 1MB recv)
   - Handshake negotiation
   - Configuration exchange
   - Keep-alive messages

2. **Stream Socket** (large buffers: 16MB send / 16MB recv)
   - Video/audio data transmission
   - Low-latency requirements
   - High bandwidth requirements

### 2. Connection State Machine

```
[App Launch]
    ↓
[OpenXR Init] → 0.2s
    ↓
[Socket Init] → ~1.5s after launch
    ↓
[Server Restart Detected] → abort & retry
    ↓
[Socket Init Retry] → 1.0s delay
    ↓
[Connection Error: EAGAIN] → ~22s timeout
    ↓
[Socket Init Retry] → 1.0s delay
    ↓
[Handshake Success] → ~10s negotiation
    ↓
[Stream Starting] → triggered by server
    ↓
[Socket Buffer Upgrade] → 16MB allocation
    ↓
[Connected to Server] → 5ms after stream start
```

### 3. Timing Patterns

| Event | Time from Launch | Delta from Previous |
|-------|-----------------|---------------------|
| Process Start | 0.000s | - |
| First Socket Init | 1.379s | +1.379s |
| Server Restart | 1.489s | +0.110s |
| Second Socket Init | 2.492s | +1.003s |
| Connection Error | 25.279s | +22.787s |
| Third Socket Init | 26.281s | +1.002s |
| **Stream Starting** | **36.436s** | **+10.155s** |
| **Connected (1st)** | **36.439s** | **+0.003s** |
| Fourth Socket Init | 39.611s | +3.172s |
| **Stream Starting** | **49.582s** | **+9.971s** |
| **Connected (2nd)** | **49.585s** | **+0.003s** |

### 4. Error Analysis

**Server Restart (09:19:37.676)**
- Occurred 110ms after client socket init
- Indicates server was in unstable state
- Possible causes:
  - Driver crash recovery
  - Configuration reload
  - Previous client cleanup incomplete

**Connection Error: Try Again (09:20:01.466)**
- POSIX error code suggests `EAGAIN`
- Socket not ready for I/O operation
- 22.8 second timeout indicates:
  - Handshake protocol has multi-second timeout
  - Server not responding to connection requests
  - Possible network buffering issue

**Microphone Errors (Server-Side)**
- "No microphones found" during handshake
- Did NOT prevent successful connections
- Suggests:
  - Audio is optional during handshake
  - Client may have disabled microphone
  - Server continues despite missing audio input

### 5. Reconnection Pattern

The client exhibited **rapid reconnection** behavior:
- First connection at 09:20:12.626
- Second connection at 09:20:25.772
- **13.1 second interval**

This suggests:
1. First connection may have been test/validation
2. First connection encountered issue and dropped
3. Second connection is the "real" streaming session
4. Automatic reconnection logic triggered

### 6. Code-Level Observations

**Socket Buffer Management** (`alvr/client_core/src/connection.rs`)
```rust
// Initial buffers
send: 524288B   (512 KB)
recv: 1048576B  (1 MB)

// Upgraded buffers (streaming)
send: 16777216B (16 MB)
recv: 16777216B (16 MB)
```

The buffer upgrade happens in the streaming socket initialization:
```rust
// Likely in alvr/sockets/src/stream_socket.rs
socket.set_send_buffer_size(16 * 1024 * 1024)?;
socket.set_recv_buffer_size(16 * 1024 * 1024)?;
```

**Connection Flow** (`alvr/client_core/src/connection.rs`)
1. Socket initialization logs "Initial socket buffer size"
2. Server sends "Stream starting" signal
3. Client upgrades socket buffers
4. Client logs "Connected to server"
5. Audio/video subsystems activate

### 7. Missing Log Data

The logs **do NOT contain**:
- Video decoder initialization messages
- Codec negotiation details
- Frame receive events
- Bitrate/quality settings
- Actual streaming data flow logs

This suggests:
- Video subsystem logging may be at different level
- Streaming may not have fully activated
- Connection established but stream not flowing
- Additional logs needed from video/decoder components

---

## Conclusions

1. **Connection Eventually Successful**: The client successfully connected twice, but after significant delays and retries.

2. **Server Instability**: The server restart event and multiple handshake errors indicate server-side issues were the primary cause of connection delays.

3. **Client Resilience**: The client demonstrated good retry logic with 1-second backoff periods and automatic reconnection.

4. **Fast Handshake**: Once conditions were met, the actual connection handshake was **extremely fast (5ms)**, suggesting the protocol is well-optimized.

5. **Audio Issues**: The "No microphones found" errors on the server side did not prevent connections, but may indicate audio streaming was unavailable.

6. **USB Connection Context**: Given the crash_log.txt shows "client.wired" errors, this appears to be a USB connection test. The handshake failures correlate with known USB connection stability issues.

---

## Recommendations

1. **Investigate Server Restart**: Determine why the server restarted at 09:19:37.676, only 110ms after client connection attempt.

2. **Reduce Handshake Timeout**: The 22.8-second timeout before error is excessive. Consider reducing to 5-10 seconds.

3. **Add Diagnostics**:
   - Log server state during handshake
   - Track microphone detection separately
   - Add video decoder initialization logging

4. **Connection Retry Logic**: Current 1-second retry delay works well, but consider exponential backoff after multiple failures.

5. **Investigate Reconnection**: Determine why a second connection was established 13 seconds after the first.

6. **Server-Side Logging**: The handshake errors suggest server-side connection handling needs investigation, particularly for USB connections.
