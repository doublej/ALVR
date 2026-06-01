# USB Connection Reliability Fixes Plan

This document outlines logging additions and fixes for each identified USB connection reliability issue.

## Confirmed by GitHub Issues

| Our Theory | Confirming Issues | Status |
|------------|-------------------|--------|
| Device state not validated | #2652 (unauthorized not handled) | ✅ Confirmed |
| Port forwarding becomes stale | #1989 ("address in use" error) | ✅ Confirmed |
| ADB server killed too frequently | #2659, #1923 (ADB lifecycle issues) | ✅ Confirmed |
| Silent handshake failures | #2185 (fixed in PR #2214, but pattern remains) | ✅ Confirmed |
| Timeouts too short | #2534 (connection timeouts) | ⚠️ Likely |

### Additional Issues Discovered from GitHub
- **#2649**: v20.12.0 regression - wired completely broken for some users
- **#3034**: Constant connect/disconnect cycles (firmware v76+ related)
- **#2630**: "Wired Client Type" config confusion causes "unknown" device
- **#3054**: xHCI crash on Linux (critical - out of scope for this fix)
- **#2716**: NixOS can't use auto-downloaded ADB (need system ADB fallback)

---

## Issue 1: Device State Not Validated

**Location**: `alvr/adb/src/lib.rs:39-47`

**Problem**: The code filters devices only by serial number, ignoring `ConnectionState`. Devices in `Unauthorized`, `Offline`, `Connecting`, or `NoPermissions` states are treated as valid.

**GitHub Evidence**: Issue #2652 - "Device doesn't Authorize" - users get no feedback when authorization is pending.

### Logging to Add

```rust
// In setup(), after list_devices() call
for device in &devices {
    dbg_connection!(
        "wired_connection: Found device serial={:?} state={:?} transport={:?}",
        device.serial, device.connection_state, device.transport_type
    );
}
```

```rust
// When device is filtered out due to bad state
warn!(
    "wired_connection: Skipping device {:?} - state is {:?} (expected Device)",
    device.serial, device.connection_state
);
```

### Fix

**File**: `alvr/adb/src/lib.rs`

Change the device selection to validate `ConnectionState`:

```rust
// Before (lines 39-47):
let Some(device_serial) = commands::list_devices(&self.adb_path)?
    .into_iter()
    .filter_map(|d| d.serial)
    .find(|s| !s.starts_with("127.0.0.1"))

// After:
let devices = commands::list_devices(&self.adb_path)?;

for device in &devices {
    dbg_connection!(
        "wired_connection: Found device serial={:?} state={:?} transport={:?}",
        device.serial, device.connection_state, device.transport_type
    );
}

let Some(device_serial) = devices
    .into_iter()
    .filter(|d| {
        // Only accept devices in "Device" state (ready for ADB commands)
        matches!(d.connection_state, Some(crate::parse::ConnectionState::Device))
    })
    .filter_map(|d| d.serial)
    .find(|s| !s.starts_with("127.0.0.1"))
else {
    // Provide more specific error messages
    let all_devices = commands::list_devices(&self.adb_path)?;
    if all_devices.is_empty() {
        return Ok(WiredConnectionStatus::NotReady("No wired devices found".to_owned()));
    }

    // Check for specific states to give helpful messages
    for device in &all_devices {
        if matches!(device.connection_state, Some(crate::parse::ConnectionState::Unauthorized)) {
            return Ok(WiredConnectionStatus::NotReady(
                "Device unauthorized - please accept USB debugging prompt on headset".to_owned()
            ));
        }
        if matches!(device.connection_state, Some(crate::parse::ConnectionState::Offline)) {
            return Ok(WiredConnectionStatus::NotReady(
                "Device offline - try reconnecting USB cable".to_owned()
            ));
        }
        if matches!(device.connection_state, Some(crate::parse::ConnectionState::NoPermissions)) {
            return Ok(WiredConnectionStatus::NotReady(
                "No permissions - check udev rules (Linux) or USB drivers (Windows)".to_owned()
            ));
        }
    }

    return Ok(WiredConnectionStatus::NotReady(
        "No ready wired devices found".to_owned()
    ));
};
```

**Also need**: Export `ConnectionState` from parse module or make it accessible.

---

## Issue 2: Port Forwarding Not Verified

**Location**: `alvr/adb/src/commands.rs:321-338`

**Problem**: Port forwarding is set up without verification that it actually works. Stale forwards can exist but be non-functional.

**GitHub Evidence**: Issue #1989 - "Address is already in use (os error 98)" - stale port forwards cause conflicts.

### Logging to Add

```rust
// After forward_port() succeeds
dbg_connection!(
    "wired_connection: Port forward created: localhost:{} -> device:{}",
    port, port
);

// When checking existing forwards
dbg_connection!(
    "wired_connection: Already forwarded ports: {:?}",
    forwarded_ports
);
```

### Fix

**File**: `alvr/adb/src/commands.rs`

Add a function to remove stale forwards and verify forwarding:

```rust
pub fn remove_forward(adb_path: &str, device_serial: &str, port: u16) -> Result<()> {
    get_command(
        adb_path,
        &[
            "-s",
            device_serial,
            "forward",
            "--remove",
            &format!("tcp:{port}"),
        ],
    )
    .output()
    .context(format!(
        "Failed to remove forward for port {port:?} of device {device_serial:?}"
    ))?;

    Ok(())
}

pub fn remove_all_forwards(adb_path: &str, device_serial: &str) -> Result<()> {
    get_command(
        adb_path,
        &["-s", device_serial, "forward", "--remove-all"],
    )
    .output()
    .context(format!(
        "Failed to remove all forwards for device {device_serial:?}"
    ))?;

    Ok(())
}
```

**File**: `alvr/adb/src/lib.rs`

Change setup() to clear stale forwards before creating new ones:

```rust
// Replace lines 49-61 with:
let forwarded_ports: HashSet<u16> =
    commands::list_forwarded_ports(&self.adb_path, &device_serial)?
        .into_iter()
        .map(|f| f.local)
        .collect();

dbg_connection!(
    "wired_connection: Currently forwarded ports for {}: {:?}",
    device_serial, forwarded_ports
);

// Remove existing forwards for our ports to ensure fresh state
let ports = HashSet::from([control_port, stream_port]);
for port in ports.intersection(&forwarded_ports) {
    dbg_connection!("wired_connection: Removing stale forward for port {}", port);
    if let Err(e) = commands::remove_forward(&self.adb_path, &device_serial, *port) {
        warn!("wired_connection: Failed to remove stale forward for port {}: {}", port, e);
    }
}

// Create fresh forwards
for port in &ports {
    commands::forward_port(&self.adb_path, &device_serial, *port)?;
    dbg_connection!(
        "wired_connection: Created port forward localhost:{} -> {}:{}",
        port, device_serial, port
    );
}
```

---

## Issue 3: Short Timeout for USB Connection

**Location**: `alvr/sockets/src/control_socket.rs:63`

**Problem**: The 1-second timeout is divided by number of IPs. USB connections through ADB have higher latency and may need more time.

### Logging to Add

```rust
// In connect_to_client(), log the timeout being used
dbg_sockets!(
    "connect_to_client: Attempting connection to {} IPs with {}ms per IP",
    client_ips.len(),
    split_timeout.as_millis()
);

// Log each connection attempt
for ip in client_ips {
    dbg_sockets!("connect_to_client: Trying {}:{}", ip, port);
    // ... existing connect code ...
    dbg_sockets!("connect_to_client: Connection to {} result: {:?}", ip, res.is_ok());
}
```

### Fix

**Option A**: Increase timeout for loopback (USB) connections

**File**: `alvr/sockets/src/control_socket.rs`

```rust
pub fn connect_to_client(
    timeout: Duration,
    client_ips: &[IpAddr],
    port: u16,
    buffer_config: SocketBufferConfig,
) -> ConResult<(TcpStream, TcpStream)> {
    let mut res = alvr_common::try_again();

    for ip in client_ips {
        // Use longer timeout for loopback (USB via ADB)
        let ip_timeout = if ip.is_loopback() {
            timeout  // Full timeout for USB
        } else {
            timeout / client_ips.len() as u32
        };

        dbg_sockets!(
            "connect_to_client: Trying {}:{} with {}ms timeout",
            ip, port, ip_timeout.as_millis()
        );

        res = TcpStream::connect_timeout(&SocketAddr::new(*ip, port), ip_timeout)
            .handle_try_again();

        if res.is_ok() {
            dbg_sockets!("connect_to_client: Connected to {}:{}", ip, port);
            break;
        } else {
            dbg_sockets!("connect_to_client: Failed to connect to {}:{}", ip, port);
        }
    }
    // ... rest unchanged
}
```

**Option B**: Pass a separate USB timeout from the caller

**File**: `alvr/server_core/src/connection.rs`

Add a longer timeout constant for wired connections:

```rust
const HANDSHAKE_ACTION_TIMEOUT: Duration = Duration::from_secs(2);
const WIRED_HANDSHAKE_ACTION_TIMEOUT: Duration = Duration::from_secs(5);
```

Then use the appropriate timeout based on connection type in `try_connect()`.

---

## Issue 4: Silent Handshake Failure

**Location**: `alvr/server_core/src/connection.rs:535-541`

**Problem**: Handshake timeouts are silently ignored for USB with a debug message, masking real failures.

**GitHub Evidence**: Issue #2185 - Connection loop breaks prematurely, fixed in PR #2214 but similar silent failure patterns remain.

### Logging to Add

Change `debug!` to `info!` or `warn!` and add more context:

```rust
Err(ConnectionError::TryAgain(e)) => {
    // Distinguish between expected retry and potential problem
    warn!(
        "wired_connection: Handshake timeout for {} (attempt may retry). \
         This can be normal for USB but repeated failures indicate a problem.\n\
         Error: {e}",
        client_hostname
    );
    return Ok(());
}
```

### Fix

**File**: `alvr/server_core/src/connection.rs`

Add retry counting to detect persistent failures:

```rust
// Add near the top of the file
const MAX_USB_HANDSHAKE_RETRIES: u32 = 5;

// In handshake_loop, track retry count per wired connection attempt
let mut wired_handshake_retries: u32 = 0;

// Inside the loop, when wired connection is ready but handshake fails
if !wired_client_ips.is_empty() {
    match try_connect(...) {
        Ok(()) => {
            // Reset on success
            wired_handshake_retries = 0;
        }
        Err(e) if is_try_again_error(&e) => {
            wired_handshake_retries += 1;
            if wired_handshake_retries >= MAX_USB_HANDSHAKE_RETRIES {
                warn!(
                    "wired_connection: {} consecutive handshake failures - \
                     clearing port forwards and retrying setup",
                    wired_handshake_retries
                );
                // Force re-setup of port forwarding
                if let Some(ref wc) = wired_connection {
                    // Clear forwards to force fresh setup next iteration
                    let _ = commands::remove_all_forwards(&wc.adb_path, &device_serial);
                }
                wired_handshake_retries = 0;
            }
        }
        Err(e) => {
            error!("wired_connection: Handshake error: {e}");
        }
    }
}
```

**File**: `alvr/server_core/src/connection.rs:535-541`

Make the code distinguish wired vs wireless in logging:

```rust
Err(ConnectionError::TryAgain(e)) => {
    let is_wired = client_ip.is_loopback();
    if is_wired {
        info!(
            "wired_connection: Handshake timeout (will retry). Error: {e}"
        );
    } else {
        debug!(
            "Handshake timeout for {} (will retry). Error: {e}",
            client_hostname
        );
    }
    return Ok(());
}
```

---

## Issue 5: ADB Server Killed on Drop

**Location**: `alvr/adb/src/lib.rs:106-113`

**Problem**: The ADB server is killed every time `WiredConnection` is dropped, which can happen during reconnection attempts, causing unnecessary restarts.

**GitHub Evidence**:
- Issue #2659 - "ADB server left open after crashes" - lifecycle management issues
- Issue #1923 - "Restarting alvr and adb may be helpful sometimes" - users manually restart ADB as workaround

### Logging to Add

```rust
impl Drop for WiredConnection {
    fn drop(&mut self) {
        info!("wired_connection: WiredConnection dropped, killing ADB server");
        // ...
    }
}
```

### Fix

**Option A**: Don't kill ADB server on drop (simplest)

```rust
impl Drop for WiredConnection {
    fn drop(&mut self) {
        dbg_connection!("wired_connection: WiredConnection dropped");
        // Don't kill ADB server - let it persist for faster reconnection
        // The server will be cleaned up when the dashboard exits
    }
}
```

Add a separate explicit cleanup method for shutdown:

```rust
impl WiredConnection {
    pub fn shutdown(&self) {
        dbg_connection!("wired_connection: Explicit shutdown, killing ADB server");
        if let Err(e) = commands::kill_server(&self.adb_path) {
            error!("wired_connection: Failed to kill ADB server: {e:?}");
        }
    }
}
```

**Option B**: Use reference counting to only kill on final drop

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

static ADB_CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);

impl WiredConnection {
    pub fn new(...) -> Result<Self> {
        // ...existing code...
        ADB_CONNECTION_COUNT.fetch_add(1, Ordering::SeqCst);
        Ok(Self { adb_path })
    }
}

impl Drop for WiredConnection {
    fn drop(&mut self) {
        let remaining = ADB_CONNECTION_COUNT.fetch_sub(1, Ordering::SeqCst) - 1;
        dbg_connection!("wired_connection: Dropped, {} connections remaining", remaining);

        if remaining == 0 {
            dbg_connection!("wired_connection: Last connection, killing ADB server");
            if let Err(e) = commands::kill_server(&self.adb_path) {
                error!("{e:?}");
            }
        }
    }
}
```

---

## Issue 6: No Retry on Stale Forwarding

**Location**: `alvr/adb/src/lib.rs:49-61`

**Problem**: Ports are only forwarded if not already in the list, but existing forwards may be stale.

### Logging to Add

Already covered in Issue 2.

### Fix

Already covered in Issue 2 (clearing stale forwards before creating new ones).

---

## Issue 7: Race Condition in Client Detection

**Location**: `alvr/adb/src/lib.rs:63-68, 70-95`

**Problem**: Multiple separate ADB calls to check client state create a window for state changes.

### Logging to Add

```rust
// Log the full state check sequence
dbg_connection!(
    "wired_connection: Checking client state for {} on {}",
    process_name, device_serial
);

let process_running = commands::get_process_id(...)?;
dbg_connection!("wired_connection: Process running: {:?}", process_running);

let activity_resumed = commands::is_activity_resumed(...)?;
dbg_connection!("wired_connection: Activity resumed: {}", activity_resumed);
```

### Fix

**Option A**: Add retry logic around state checks

**File**: `alvr/adb/src/lib.rs`

```rust
// After all checks pass, verify once more before returning Ready
if commands::get_process_id(&self.adb_path, &device_serial, &process_name)?.is_some()
    && commands::is_activity_resumed(&self.adb_path, &device_serial, &process_name)?
{
    dbg_connection!("wired_connection: Client verified ready");
    Ok(WiredConnectionStatus::Ready)
} else {
    dbg_connection!("wired_connection: Client state changed during setup, retrying");
    Ok(WiredConnectionStatus::NotReady(
        "Client state changed during setup".to_owned()
    ))
}
```

**Option B**: Combine checks into single ADB call (more complex)

Create a new ADB command that checks multiple things at once:

```rust
// In commands.rs
pub fn get_app_state(
    adb_path: &str,
    device_serial: &str,
    package_name: &str,
) -> Result<AppState> {
    // Single shell command that outputs both process ID and activity state
    let output = get_command(
        adb_path,
        &[
            "-s", device_serial, "shell",
            &format!(
                "pidof {} && dumpsys activity {} | grep mResumed",
                package_name, package_name
            ),
        ],
    )
    .output()?;

    // Parse combined output
    // ...
}
```

---

## Issue 8: Wired Client Type Mismatch Not Detected (NEW)

**Location**: `alvr/adb/src/lib.rs:115-151` (get_process_name function)

**Problem**: When "Wired Client Type" setting doesn't match the installed client (Store vs GitHub), connection silently fails with unhelpful "unknown" device status.

**GitHub Evidence**: Issue #2630 - Users confused by "Wired Client Type" setting, took significant troubleshooting to realize Store/GitHub mismatch.

### Logging to Add

```rust
// In get_process_name(), log which packages are being checked
for name in &fallbacks {
    let installed = commands::is_package_installed(adb_path, device_serial, name)
        .is_ok_and(|installed| installed);
    dbg_connection!(
        "wired_connection: Package {} installed: {}",
        name, installed
    );
}
```

### Fix

**File**: `alvr/adb/src/lib.rs`

Improve error messages when no matching client is found:

```rust
pub fn get_process_name(
    adb_path: &str,
    device_serial: &str,
    flavor: &ClientFlavor,
) -> Option<String> {
    let fallbacks = match flavor {
        // ... existing match arms ...
    };

    // Log what we're looking for
    dbg_connection!(
        "wired_connection: Looking for client with flavor {:?}, checking packages: {:?}",
        flavor, fallbacks
    );

    // Check all packages and collect results for better error reporting
    let mut found_packages = Vec::new();
    for name in &fallbacks {
        if commands::is_package_installed(adb_path, device_serial, name)
            .is_ok_and(|installed| installed)
        {
            found_packages.push(*name);
        }
    }

    if found_packages.is_empty() {
        // Check if OTHER client types are installed to give helpful hint
        let all_packages = [PACKAGE_NAME_STORE, PACKAGE_NAME_GITHUB_STABLE, PACKAGE_NAME_GITHUB_DEV];
        let installed_others: Vec<_> = all_packages.iter()
            .filter(|name| !fallbacks.contains(name))
            .filter(|name| commands::is_package_installed(adb_path, device_serial, name)
                .is_ok_and(|installed| installed))
            .collect();

        if !installed_others.is_empty() {
            warn!(
                "wired_connection: No matching client for {:?}, but found: {:?}. \
                 Try changing 'Wired Client Type' in Connection settings.",
                flavor, installed_others
            );
        }

        return None;
    }

    Some(found_packages[0].to_string())
}
```

---

## Implementation Order

Recommended order of implementation (by impact and risk):

1. **Issue 1** (Device State Validation) - High impact, low risk - **Fixes #2652**
2. **Issue 2** (Port Forward Verification) - High impact, medium risk - **Fixes #1989**
3. **Issue 8** (Client Type Mismatch) - High impact, low risk - **Fixes #2630** (NEW)
4. **Issue 4** (Silent Handshake Failure) - Medium impact, low risk - **Improves #2185**
5. **Issue 5** (ADB Server Kill) - Medium impact, low risk - **Fixes #2659, #1923**
6. **Issue 3** (Timeout) - Medium impact, low risk - **May help #2534**
7. **Issue 7** (Race Condition) - Low impact, medium risk

---

## Testing Plan

After implementing each fix:

1. Test basic USB connection flow
2. Test reconnection after cable disconnect/reconnect
3. Test connection when device is in unauthorized state
4. Test connection after headset reboot
5. Test rapid connect/disconnect cycles
6. Test with multiple devices connected (if applicable)
7. Verify log output is helpful for debugging
8. Test with mismatched "Wired Client Type" setting (NEW)
9. Test connection with WiFi disabled (regression test for #2376)

---

## Out of Scope

These issues were identified but are outside the scope of this fix:

- **#3054**: xHCI host controller crash on Linux - kernel/driver level issue
- **#3034**: Connect/disconnect loops related to Quest firmware v76+ - firmware issue
- **#2716**: NixOS ADB path issue - requires architectural change to support system ADB
- **#2770**: VB-Cable driver conflict on Windows - third-party software issue
