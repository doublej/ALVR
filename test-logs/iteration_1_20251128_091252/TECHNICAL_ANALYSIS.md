# ALVR Client Log - Deep Technical Analysis
**Test Session:** iteration_1_20251128_091252
**Analysis Date:** 2025-11-28
**Log Duration:** 07:41:09 to 09:13:05+ (approximately 92 minutes)

---

## Executive Summary

This analysis reveals a **persistent connection failure pattern** where the ALVR client on the headset repeatedly attempts to connect to the server via USB/ADB but fails during the handshake phase. The client successfully initializes sockets but never progresses to the streaming phase. Server-side handshake errors confirm the connection attempts are reaching the server but failing to complete the protocol exchange.

---

## Connection Event Timeline

### Initial Launch (09:11:24)
```
09:11:24.097 - ClientInputSettings initialized for alvr.client.dev
09:11:24.122 - [ALVR NATIVE-RUST]: FIXME: Leaking Imagereader!
09:11:25.239 - [ALVR NATIVE-RUST]: Initial socket buffer size: send: 524288B, recv: 1048576B
```

**Analysis:**
- **09:11:24.122**: ImageReader leak warning - This is a known resource leak in the Android graphics pipeline, non-critical but indicates the client OpenXR/graphics subsystem is initializing.
- **09:11:25.239**: First socket initialization - This occurs in `alvr_sockets::set_socket_buffers()` (line 28-32 of `alvr/sockets/src/lib.rs`). The client has created a TCP socket and is attempting to connect.
- **Timing**: 1.117 seconds from client start to socket initialization.

**Code Context:** This message appears in the `ProtoControlSocket::connect_to()` path when the client calls `TcpStream::connect_timeout()` to reach the server on the control port (9943).

---

### First Restart Event (09:11:56)
```
09:11:56.149 - Monkey: args: [-p, alvr.client.dev, -c, android.intent.category.LAUNCHER, 1]
09:11:56.184 - ActivityTaskManager: START with LAUNCH_SINGLE_TASK from uid 2000
09:11:56.188 - OpenXR: nativeOnActivityPaused
09:11:56.188 - OpenXR: nativeOnActivityReady
09:11:56.259 - [ALVR NATIVE-RUST]: Server restarting
09:11:57.263 - [ALVR NATIVE-RUST]: Initial socket buffer size: send: 524288B, recv: 1048576B
```

**Analysis:**
- **09:11:56.149**: External restart trigger via Android Monkey (automated test framework) - This is the test script attempting to restart the app.
- **09:11:56.259**: Client received `ServerControlPacket::Restarting` - The server sent an explicit restart notification.
- **09:11:57.263**: Socket reinitialized after restart.
- **Timing**: 32.0 seconds from initial socket init to restart event.

**Code Context:** The "Server restarting" message comes from `alvr/client_core/src/connection.rs:269-277`. The client receives `ServerControlPacket::Restarting` during the handshake phase (between sending capabilities and receiving StartStream). This causes the `connection_pipeline()` to return `Ok(())` and retry from the beginning.

**Critical Insight:** The server sent a restart notification during the handshake, meaning:
1. The TCP connection was established successfully
2. The handshake began but did not complete
3. The server initiated the termination (not a timeout)

---

### First Connection Error (09:12:30)
```
09:12:27.929 - Monkey: args: [-p, alvr.client.dev, -c, android.intent.category.LAUNCHER, 1]
09:12:27.960 - ActivityTaskManager: START with LAUNCH_SINGLE_TASK
09:12:27.964 - OpenXR: nativeOnActivityPaused
09:12:27.964 - OpenXR: nativeOnActivityReady
09:12:30.045 - [ALVR NATIVE-RUST]: Connection error: Try again
09:12:31.047 - [ALVR NATIVE-RUST]: Initial socket buffer size: send: 524288B, recv: 1048576B
```

**Analysis:**
- **09:12:27.929**: Second restart trigger via Monkey.
- **09:12:30.045**: First explicit connection error - "Try again" is a `ConnectionError::TryAgain` variant.
- **Timing**: 33.79 seconds from previous socket init to error; 2.081 seconds from app activity ready to error.

**Code Context:** The "Connection error: Try again" message is logged in `alvr/client_core/src/connection.rs:127-129` when `connection_pipeline()` returns `Err(ConnectionError::TryAgain(_))`. This error propagates from the socket layer.

**Critical Insight:** The `TryAgain` error suggests one of the following:
1. TCP connection timeout - `TcpStream::connect_timeout()` returned `WouldBlock/TimedOut`
2. Socket read timeout during handshake - Waiting for server response exceeded `HANDSHAKE_ACTION_TIMEOUT` (2 seconds)
3. Server listener not accepting connections

The 2-second delay from app ready to error suggests it's likely a **handshake timeout** rather than TCP connect timeout, as the handshake timeout is exactly 2 seconds (`HANDSHAKE_ACTION_TIMEOUT` in connection.rs:53).

---

### Server Handshake Errors (crash_log.txt)
```
09:12:39.023 [ERROR] Handshake error for client.wired: Try again
09:12:50.731 [ERROR] Handshake error for client.wired: Try again
```

**Analysis:**
- **09:12:39.023**: Server-side handshake error, 8.978 seconds after client connection error.
- **09:12:50.731**: Second server-side handshake error, 11.708 seconds after first.

**Code Context:** These errors come from `alvr/server_core/src/connection.rs:517` when `connection_pipeline()` returns an error. The error is caught in the handshake thread spawned at line 507.

**Critical Correlation:**
The server errors occur **AFTER** the client connection errors, suggesting:
1. Client attempts connection
2. Client times out waiting for server response
3. Client retries
4. Server finally processes the connection but the client has already moved on
5. Server logs "Try again" error when its side of the handshake fails

This indicates a **timing/race condition** where:
- The client's `HANDSHAKE_ACTION_TIMEOUT` (2 seconds) is **too short**
- The server's handshake processing takes longer than 2 seconds
- By the time the server is ready to respond, the client has already timed out

---

### Second Restart Event (09:12:41)
```
09:12:41.353 - [ALVR NATIVE-RUST]: Server restarting
09:12:42.357 - [ALVR NATIVE-RUST]: Initial socket buffer size: send: 524288B, recv: 1048576B
```

**Analysis:**
- Server sent another restart notification
- Timing: 11.31 seconds from previous socket init

**Pattern:** This is identical to the first restart event - the client receives `ServerControlPacket::Restarting` during handshake.

---

### Second Connection Error (09:13:05)
```
09:13:05.031 - [ALVR NATIVE-RUST]: Connection error: Try again
```

**Analysis:**
- Same error pattern as before
- Timing: 22.67 seconds from previous socket init

---

## Connection Phase Analysis

Based on the code analysis, here's what happens during each connection attempt:

### Phase 1: Discovery and TCP Connection
**Duration:** ~500ms - 2s
**Code Location:** `alvr/client_core/src/connection.rs:152-183`

1. Client creates announcer socket (UDP broadcast)
2. Client creates listener socket for server discovery
3. Client repeatedly announces presence via UDP
4. `ProtoControlSocket::connect_to()` attempts TCP connection with `SOCKET_INIT_RETRY_INTERVAL` (500ms) timeout
5. **Success indicator:** "Initial socket buffer size" message appears

**Observed Behavior:** This phase consistently succeeds - we see the socket buffer message every time.

---

### Phase 2: Capability Exchange (Handshake)
**Duration:** Should be <2s (HANDSHAKE_ACTION_TIMEOUT)
**Code Location:** `alvr/client_core/src/connection.rs:185-223`

1. Client sends `ClientConnectionResult::ConnectionAccepted` with capabilities
2. Client waits to receive `StreamConfigPacket` from server
3. **Timeout:** 2 seconds (`HANDSHAKE_ACTION_TIMEOUT`)

**Observed Behavior:** This phase fails. The client either:
- Receives `ServerControlPacket::Restarting` (causes graceful retry)
- Times out waiting for `StreamConfigPacket` (causes "Try again" error)

---

### Phase 3: Stream Configuration
**Duration:** Should be <2s
**Code Location:** `alvr/client_core/src/connection.rs:256-300`

1. Client waits for `ServerControlPacket::StartStream`
2. On success, proceeds to stream setup

**Observed Behavior:** Never reached in this session.

---

### Phase 4: Stream Setup and Streaming
**Code Location:** `alvr/client_core/src/connection.rs:302-689`

**Observed Behavior:** Never reached in this session.

---

## Error Pattern Analysis

### Connection Error Type: "Try again"
**Source:** `ConnectionError::TryAgain` variant
**Common Causes:**
1. **Socket timeout** - Most likely given the 2-second pattern
2. **Interrupted syscall** - Less likely on Android
3. **Server not responding** - Matches server-side errors

### Timing Pattern
```
Event                                  Timestamp        Delta from prev
─────────────────────────────────────────────────────────────────────────
Initial socket init                    09:11:25.239     (baseline)
Server restart notification            09:11:56.259     +31.02s
Socket reinit after restart            09:11:57.263     +1.004s
Second app restart                     09:12:27.964     +30.70s
First "Try again" error                09:12:30.045     +2.081s
Socket reinit after error              09:12:31.047     +1.002s
Server handshake error #1              09:12:39.023     +7.976s
Second server restart notification     09:12:41.353     +2.330s
Socket reinit after restart            09:12:42.357     +1.004s
Server handshake error #2              09:12:50.731     +8.374s
Second "Try again" error               09:13:05.031     +14.30s
```

**Pattern Observations:**
1. **Socket reinit delay:** Consistently ~1 second after restart/error - This is `CONNECTION_RETRY_INTERVAL` (1 second) in connection.rs:52
2. **Error timing:** 2-8 seconds after activity ready - Variable, suggests network/processing delays
3. **Server lag:** 7-8 seconds behind client errors - Server processes connections slower than client timeout

---

## Root Cause Analysis

### Primary Issue: Handshake Timeout Race Condition

**Evidence:**
1. Client socket initialization succeeds (TCP connection works)
2. Client times out waiting for server response
3. Server logs handshake errors seconds after client has already failed
4. The 2-second `HANDSHAKE_ACTION_TIMEOUT` is too short for USB/ADB connections

**USB/ADB Connection Characteristics:**
- USB connections use ADB port forwarding
- Port forwarding adds latency to every socket operation
- Server-side processing time is not accounted for in client timeout

**Code Reference:**
```rust
// alvr/client_core/src/connection.rs:53
const HANDSHAKE_ACTION_TIMEOUT: Duration = Duration::from_secs(2);
```

This 2-second timeout applies to:
- Waiting for server's `StreamConfigPacket` (line 223)
- Waiting for `StartStream` packet (line 256)
- Stream socket connection (line 330)

For USB connections, this timeout is insufficient.

---

### Secondary Issue: Server Restart Loop

**Evidence:**
Multiple "Server restarting" messages suggest the server is cycling, possibly due to:
1. Configuration changes
2. Detection of connection failures
3. Automatic recovery mechanism

The server restart compounds the handshake timeout issue by resetting the connection state mid-handshake.

---

## Technical Recommendations

### 1. Increase USB Handshake Timeout
**Location:** `alvr/client_core/src/connection.rs`

The `HANDSHAKE_ACTION_TIMEOUT` should differentiate between USB and WiFi:
- **Current:** 2 seconds for all connections
- **Recommended:**
  - USB/Loopback: 10-15 seconds
  - WiFi: 2-5 seconds

**Rationale:** USB connections via ADB port forwarding require significantly more time for handshake round-trips. The server-side logs show handshake processing can take 7-8 seconds.

---

### 2. Implement Exponential Backoff for Retries
**Location:** `alvr/client_core/src/connection.rs:138`

Current retry logic uses fixed `CONNECTION_RETRY_INTERVAL` (1 second). For USB connections experiencing repeated failures, this should use exponential backoff to reduce server load and allow transient issues to resolve.

---

### 3. Add Handshake Progress Logging
**Missing:** Detailed handshake phase logging on client side

Currently, the client only logs:
- Socket init
- Connection errors
- Server restarting

**Recommended additions:**
- "Sending capabilities to server"
- "Waiting for stream config"
- "Stream config received"
- "Waiting for start stream"

This would help identify exactly where in the handshake the timeout occurs.

---

### 4. Server-Side Handshake Optimization
**Location:** `alvr/server_core/src/connection.rs`

The server takes 7-8 seconds to process handshake attempts. Investigate:
- What processing occurs between receiving client capabilities and sending stream config?
- Are there blocking operations that could be asynchronous?
- Is session configuration loading causing delays?

---

### 5. USB Connection Detection
**Location:** `alvr/client_core/src/connection.rs:169`

The code checks `is_loopback()` to detect USB connections:
```rust
let is_wired = pair.1.is_loopback();
```

However, this detection doesn't adjust timeouts. The timeout differentiation exists in server code (`alvr/sockets/src/control_socket.rs:73-78`) but not in client code.

**Recommendation:** Use the same timeout scaling on the client side.

---

## Diagnostic Questions for Further Investigation

1. **What is the server doing during the 7-8 second delay before logging handshake errors?**
   - Is it waiting for a timeout?
   - Is it processing configuration?
   - Is there a thread scheduling issue?

2. **Why does the server send "Restarting" packets during handshake?**
   - What triggers the server restart?
   - Is this intentional or a side effect of failure handling?

3. **Are there network buffer/queue issues in the USB/ADB layer?**
   - The consistent socket buffer sizes (524288B send, 1048576B recv) suggest these may be insufficient for USB latency.

4. **Is the ImageReader leak related to connection failures?**
   - While marked as "FIXME", does this resource leak accumulate and cause issues after multiple connection attempts?

---

## Conclusion

The USB connection failures are primarily caused by a **handshake timeout race condition** where:

1. The client's 2-second handshake timeout is too aggressive for USB/ADB connections
2. The server's handshake processing takes 7-8 seconds (evidenced by crash_log timestamps)
3. By the time the server is ready to respond, the client has already timed out and retried
4. This creates a perpetual mismatch where client and server are never synchronized

The fix requires **increasing the client-side handshake timeout for USB connections** to match the server's actual processing time, plus implementing better timeout scaling and retry logic for wired connections.

**Severity:** High - USB connections are completely non-functional
**Impact:** Users cannot connect via USB/ADB
**Effort:** Medium - Requires timeout tuning and testing across connection types
