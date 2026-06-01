# ALVR USB Connection Test - Analysis Report
## Iteration 1 - November 28, 2025 09:12:52

---

## Executive Summary

**STATUS: FAILED - Handshake Timeout**

The USB/wired connection test failed with repeated handshake timeout errors. Despite the fix to remove blocking audio initialization during handshake, the client and server were unable to complete the handshake process within the timeout period.

### Key Findings:
- **No successful connection established**
- **2 handshake timeout errors** on server side
- **2 "Try again" connection errors** on client side
- **No streaming occurred** - no audio/video data exchanged
- **Client remained running** throughout test period without crashes
- **Server driver loaded successfully** but could not complete handshake

---

## Timeline of Events

### Initial Client Start (First Attempt)
- **09:11:24.122** - Client started, ALVR NATIVE-RUST initialized
- **09:11:24.235** - Binder warnings (Android-specific, non-critical)
- **09:11:25.239** - Initial socket buffer size configured (send: 524288B, recv: 1048576B)
- **~09:11:25 - 09:11:56** - Client waiting/idle (no connection attempts logged)

### First Client Restart
- **09:11:56.149** - Test script triggered first client restart via Monkey
- **09:11:56.188** - Activity paused/ready cycle
- **09:11:56.259** - Server restarting message
- **09:11:57.263** - Socket buffers reinitialized
- **~09:11:57 - 09:12:27** - Waiting period (no connection)

### Second Client Restart & First Connection Attempt
- **09:12:27.929** - Test script triggered second client restart
- **09:12:27.964** - Activity paused/ready cycle
- **09:12:30.045** - **CLIENT ERROR: "Connection error: Try again"**
- **09:12:31.047** - Socket buffers reinitialized after error

### Second Connection Attempt
- **09:12:41.353** - Server restarting
- **09:12:42.357** - Socket buffers reinitialized
- **~09:12:42 - 09:13:05** - Another waiting/retry period
- **09:13:05.031** - **CLIENT ERROR: "Connection error: Try again"** (second occurrence)

### Server-Side Errors (from crash_log.txt)
- **09:12:39.023** - [ERROR] Handshake error for client.wired: Try again
- **09:12:50.731** - [ERROR] Handshake error for client.wired: Try again

### Test Period End
- **09:13:00.353** - VR server still running (last vrserver.txt entry)
- Test terminated shortly after

---

## Error Analysis

### Handshake Timeout Pattern

The errors follow a consistent pattern:

1. **Client-side**: "Connection error: Try again"
   - First occurrence: 09:12:30.045
   - Second occurrence: 09:13:05.031
   - Interval: ~35 seconds between errors

2. **Server-side**: "Handshake error for client.wired: Try again"
   - First occurrence: 09:12:39.023
   - Second occurrence: 09:12:50.731
   - Interval: ~11.7 seconds between errors

### Timing Discrepancy

There's a notable discrepancy between client and server error timestamps:
- Client error at 09:12:30.045, but server doesn't log error until 09:12:39.023 (9 seconds later)
- This suggests the handshake attempt begins on client, but server timeout/error detection is delayed

### Root Cause Analysis

The "Try again" error typically indicates:
1. **Network socket would block** - The socket operation couldn't complete immediately (EAGAIN/EWOULDBLOCK)
2. **Timeout during handshake negotiation** - Handshake protocol not completing within expected time
3. **Packet loss or ADB forwarding issues** - USB/ADB connection may be dropping packets

Key observations:
- Multiple "Server restarting" messages suggest connection loop/retry logic
- Socket buffers are being reinitialized repeatedly
- No evidence of successful handshake completion
- No audio/video streaming started
- No "Connected to server" messages found in logs

---

## Connection State Summary

### Client State
- **Connection attempts**: At least 2 documented
- **Socket initialization**: 4 times (09:11:25, 09:11:57, 09:12:31, 09:12:42)
- **Restarts**: 2 explicit restarts via test script
- **Errors**: 2 "Connection error: Try again"
- **Final state**: Still running but disconnected

### Server State
- **Driver loaded**: Yes (alvr_server driver activated at 09:11:49.807)
- **HMD registered**: Yes (1WMHH000X00000)
- **Handshake errors**: 2 documented
- **Client discovery**: Not explicitly logged (wired client expected)
- **Final state**: Running but no client connected

### Session Configuration
- **Connection mode**: Wired (client.wired)
- **Auto-launch enabled**: Yes (boot_delay: 0)
- **Client type**: Github variant
- **Client trusted**: Yes
- **Stream protocol**: UDP
- **Audio enabled**: Game audio enabled, microphone disabled

---

## Network/Socket Analysis

### Socket Configuration (consistent across attempts)
- **Send buffer**: 524,288 bytes (512 KB)
- **Receive buffer**: 1,048,576 bytes (1 MB)
- **Stream port**: 9944
- **Web server port**: 8082
- **Packet size**: 1400 bytes

### Buffer Settings
- **Server buffer config**: Maximum
- **Client buffer config**: Maximum
- **Max queued frames**: 1024

---

## VR Server Status

The SteamVR server (vrserver.txt) shows normal startup behavior:
- **Start time**: 09:11:48.300
- **Driver initialization**: Successful
- **ALVR driver loaded**: From `C:\Users\jurre\PycharmProjects\ALVR\build\alvr_streamer_windows\bin\win64\driver_alvr_server.dll`
- **HMD activated**: alvr_server.1WMHH000X00000
- **Warnings/Errors**: Only standard SteamVR warnings (remapping, action manifests) - not ALVR-specific
- **No ALVR-specific connection logs**: VR server doesn't show handshake attempts in detail

---

## Comparison to Previous Behavior

Based on the test context (audio initialization fix):

### Expected Improvement
The fix removed blocking audio initialization from the handshake phase, which should have:
- Reduced handshake time
- Prevented audio-related timeouts during initial connection

### Actual Result
- **Still experiencing handshake timeouts**
- **Different error**: Now seeing generic "Try again" instead of audio-specific errors
- **Suggests**: Audio fix may have worked, but revealed underlying connection issue

---

## Recommendations for Further Investigation

### 1. ADB Connection Stability
**Priority: HIGH**

The "Try again" errors suggest network-level issues. Investigate:
- Check ADB connection quality: `adb devices -l`
- Test ADB port forwarding: Verify ports are forwarded correctly
- USB cable quality: Try different cable/port
- Check for ADB server restarts during test

**Action items**:
```bash
# Verify ADB is stable
adb devices
adb shell "ping -c 10 localhost"

# Check port forwarding
adb forward --list
```

### 2. Handshake Timeout Value
**Priority: MEDIUM**

Current timeouts may be too aggressive for USB/ADB connection latency.

**Action items**:
- Locate handshake timeout configuration in code
- Consider increasing timeout for wired connections
- Add more detailed logging during handshake phases

### 3. Handshake Protocol Tracing
**Priority: HIGH**

Need detailed logging to see where handshake fails.

**Action items**:
- Enable debug logging for connection/handshake phase
- Add timestamps for each handshake step
- Log packet send/receive during handshake
- Check for differences between wired vs wireless handshake paths

**Suggested debug groups to enable in session.json**:
```json
"debug_groups": {
  "connection": true,
  "sockets": true,
  "server_core": true,
  "client_core": true
}
```

### 4. Socket Non-Blocking Behavior
**Priority: MEDIUM**

The "Try again" error indicates non-blocking socket hitting EAGAIN/EWOULDBLOCK.

**Action items**:
- Review socket configuration for wired connections
- Check if socket timeout/retry logic is appropriate
- Consider if wired connections should use different socket settings than WiFi

### 5. Client Discovery for Wired Connections
**Priority: MEDIUM**

No explicit client discovery messages for wired client.

**Action items**:
- Verify wired client discovery mechanism
- Check if client.wired registration happens differently
- Confirm ADB device is properly detected

---

## Test Environment Details

### Client (Android Headset)
- **Device**: Quest 3
- **Client ID**: client.wired
- **Process ID**: 19270
- **Display name**: Quest 3
- **Trust status**: Trusted
- **OpenXR**: Active and responding normally

### Server (Windows PC)
- **ALVR Version**: 21.0.0-dev12
- **Build path**: C:\Users\jurre\PycharmProjects\ALVR\build\alvr_streamer_windows\
- **Driver**: alvr_server (IServerTrackedDeviceProvider_004)
- **SteamVR**: Running and operational

### Test Configuration
- **Connection type**: USB via ADB
- **Auto-launch**: Enabled (boot_delay: 0)
- **Client restarts**: 2 during test (via Monkey tool)
- **Test duration**: ~2 minutes (09:11:24 - 09:13:05)

---

## Logs Analyzed

1. **crash_log.txt**: 2 handshake errors
2. **alvr_logcat.txt**: 778 lines, client-side ALVR logs
3. **client_logcat.txt**: Full Android logcat (limited analysis due to size)
4. **vrserver.txt**: 1746 lines, SteamVR driver logs
5. **session.json**: Configuration snapshot
6. **analysis.json**: Test metadata

---

## Next Steps

1. **Immediate**: Enable detailed connection debugging
   - Set connection, sockets, server_core, client_core debug flags to true
   - Re-run test to capture detailed handshake protocol messages

2. **Short-term**: Investigate ADB connection stability
   - Test with different USB cables/ports
   - Check ADB version compatibility
   - Monitor ADB daemon behavior during connection

3. **Medium-term**: Review handshake timeout configuration
   - Identify timeout values in source code
   - Consider separate timeouts for wired vs wireless
   - Add progressive retry logic with backoff

4. **Code review**: Examine wired connection handshake path
   - File: `C:\Users\jurre\PycharmProjects\ALVR\alvr\server_core\src\connection.rs`
   - File: `C:\Users\jurre\PycharmProjects\ALVR\alvr\client_core\src\connection.rs`
   - Look for handshake state machine and timeout handling

---

## Conclusion

While the audio initialization fix may have resolved the original blocking issue, the connection still fails at the handshake stage with network-level "Try again" errors. The handshake protocol is timing out before completion, likely due to:

1. ADB connection latency/stability issues
2. Insufficient timeout values for USB connections
3. Potential packet loss in ADB forwarding layer
4. Missing or incomplete handshake protocol steps

The consistent ~11-35 second intervals between errors suggest a timeout mechanism is triggering, but the handshake isn't progressing far enough to complete. More detailed logging is needed to pinpoint the exact failure point in the handshake sequence.

**Overall Assessment**: The test reveals a deeper connection stability issue that requires investigation of the ADB transport layer and handshake protocol timing, beyond the audio initialization fix that was applied.
