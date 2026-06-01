# ALVR Log Analysis Guide

## Quick Reference: What to Look For

### 1. Connection Issues

**Keywords to search:**
- `Handshake` - Connection negotiation
- `Try again` - Failed connection attempt
- `Connection ready` - Successful connection start
- `socket` - Network issues
- `timeout` - Connection timeouts

**Healthy connection flow:**
```
Connection ready for device XXXX
Initial socket buffer size: send: 65536B, recv: 65536B
[client connects]
Streaming started
```

**Problem indicators:**
```
Failed to receive client connection packet
Handshake error for client.wired: Try again
Socket error: Connection refused
```

### 2. Video/Encoder Issues

**Keywords:**
- `encoder` - Encoder initialization/errors
- `NvEnc` / `AMF` / `VAAPI` - Hardware encoder specific
- `frame` - Frame processing
- `bitrate` - Bandwidth issues
- `resolution` - Resolution problems

**Problem indicators:**
```
Encoder initialization failed
Failed to encode frame
Hardware encoder not available
Bitrate too high for connection
```

### 3. Tracking Issues

**Keywords:**
- `tracking` - General tracking
- `pose` - Head/controller position
- `controller` - Controller specific
- `latency` - Timing issues

**Problem indicators:**
```
Tracking lost
Invalid pose data
Controller not found
High tracking latency
```

### 4. Audio Issues

**Keywords:**
- `audio` - Audio pipeline
- `microphone` - Mic issues
- `playback` - Speaker output
- `sample rate` - Audio format

### 5. Crashes/Panics

**Keywords:**
- `panic` - Rust panic
- `FATAL` - Fatal errors
- `crash` - Crash indicators
- `Exception` - Exceptions
- `segfault` / `access violation` - Memory issues

---

## Log Sources

### Streamer Logs (PC)

| File | Contains |
|------|----------|
| `alvr_logs.txt` | Main ALVR server logs |
| `crash_log.txt` | Crash information |
| `session.json` | Configuration state |
| `vrserver.txt` | SteamVR logs |

### Client Logs (Quest)

| Source | Command |
|--------|---------|
| ALVR client | `adb logcat -s "alvr_client"` |
| OpenXR | `adb logcat -s "OpenXR"` |
| All ALVR | `adb logcat \| grep -i alvr` |

---

## Common Issues & Solutions

### Issue: "Handshake error: Try again"
**Look for:** Socket/network errors preceding the handshake
**Cause:** Client not ready or network issue
**Fix:** Ensure client app is open, check firewall

### Issue: Connection drops
**Look for:** `Socket error`, `timeout`, `disconnected`
**Cause:** Network instability, WiFi interference
**Fix:** Check network quality, use 5GHz WiFi

### Issue: Black screen after connect
**Look for:** Encoder errors, frame errors
**Cause:** GPU driver issue, encoder compatibility
**Fix:** Update GPU drivers, try different encoder

### Issue: High latency
**Look for:** `latency` values, frame timing
**Cause:** Network congestion, encoder too slow
**Fix:** Lower bitrate, check network

### Issue: Controller tracking lost
**Look for:** `controller`, `tracking`, `pose`
**Cause:** Client-side tracking issue
**Fix:** Check Quest tracking, lighting conditions

---

## Log Severity Levels

| Level | Meaning |
|-------|---------|
| `DEBUG` | Verbose debugging info |
| `INFO` | Normal operation events |
| `WARN` | Potential issues |
| `ERROR` | Errors that may affect operation |
| `FATAL` | Critical errors, likely crash |

---

## Analysis Workflow

1. **Start with errors**: `grep -i "error\|fatal\|panic" alvr_logs.txt`
2. **Check timeline**: Find when issue occurred
3. **Look backwards**: What happened before the error?
4. **Check both sides**: Server AND client logs
5. **Compare with working**: What's different from successful runs?

---

## Useful grep commands

```bash
# Find all errors
grep -i "error\|fatal\|panic" alvr_logs.txt

# Connection issues
grep -i "handshake\|connection\|socket" alvr_logs.txt

# Encoder issues
grep -i "encoder\|nvenc\|frame" alvr_logs.txt

# Timing/latency
grep -i "latency\|timeout\|delay" alvr_logs.txt

# Client logcat errors
adb logcat -d | grep -i "alvr.*error"
```

---

## Test Loop Output Structure

Each test iteration creates:
```
test-logs/
  iteration_1_20241128_123456/
    alvr_diagnostics.json    # Full diagnostics from API
    alvr_logs.txt            # Human-readable server logs
    session.json             # Server configuration
    crash_log.txt            # If crash occurred
    client_logcat.txt        # Quest client logs
    alvr_logcat.txt          # ALVR-filtered logcat
    vrserver.txt             # SteamVR logs
    analysis.json            # Automated analysis results
```
