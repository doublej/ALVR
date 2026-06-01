# ALVR Diagnostics Feature Plan

## Overview

Add a unified diagnostics system that aggregates logs from multiple sources (streamer, SteamVR, ADB, client) with real-time streaming via WebSocket. The feature will have both a Web UI and a Dashboard Tab.

## Architecture

```
                    +------------------+
                    |   Web Browser    |
                    |  /diagnostics    |
                    +--------+---------+
                             |
                    WebSocket (real-time)
                             |
+----------------+  +--------v---------+  +----------------+
| Dashboard Tab  |  |   Web Server     |  | DiagService    |
| (egui)         |  |   (Axum)         |  | (Background)   |
+-------+--------+  +--------+---------+  +-------+--------+
        |                    |                    |
        +--------------------+--------------------+
                             |
              +-----------------------------+
              |       Event Broadcast       |
              |  (tokio::broadcast channel) |
              +-----------------------------+
                             ^
         +-------------------+-------------------+
         |                   |                   |
+--------+--------+ +--------+--------+ +--------+--------+
| Streamer Logs   | | SteamVR Logs    | | ADB/Client      |
| (existing)      | | (file tail)     | | (logcat stream) |
+-----------------+ +-----------------+ +-----------------+
```

## Data Sources

### 1. Streamer Logs (Already Exists)
- Source: `EVENTS_SENDER` broadcast channel
- Events: `EventType::Log`, `EventType::DebugGroup`
- No changes needed - reuse existing infrastructure

### 2. SteamVR Logs (New)
- Source: `{Steam}/logs/vrserver.txt`
- Method: File tail with `notify` crate or polling
- Windows: Find via registry or common paths
- Linux: `~/.steam/steam/logs/vrserver.txt`

### 3. ADB Status (New)
- Source: `alvr/adb/src/commands.rs` functions
- Data:
  - Device list with connection states
  - Port forward status
  - Package installation status
- Method: Periodic polling (every 2-3 seconds)

### 4. Client Logcat (New)
- Source: `adb logcat -s alvr` subprocess
- Method: Spawn long-running process, stream stdout
- Filter: `[ALVR NATIVE-RUST]` tag

## Implementation Plan

### Phase 1: Backend Infrastructure

#### 1.1 Create DiagnosticsService (`alvr/server_core/src/diagnostics.rs`)

```rust
pub struct DiagnosticsService {
    adb_path: Option<String>,
    logcat_process: Option<Child>,
    events_sender: broadcast::Sender<DiagEvent>,
}

pub enum DiagSource {
    Streamer,
    SteamVR,
    Adb,
    Client,
}

pub struct DiagEvent {
    pub timestamp: String,
    pub source: DiagSource,
    pub level: LogSeverity,
    pub message: String,
}

pub enum AdbStatus {
    NotInstalled,
    NoDevices,
    DeviceFound {
        serial: String,
        state: ConnectionState,
        ports_forwarded: Vec<u16>,
        client_installed: Option<String>,
        client_running: bool,
    },
}
```

Functions:
- `new()` - Initialize service
- `start()` - Start background tasks
- `stop()` - Stop all background tasks
- `get_adb_status()` - Get current ADB/device status
- `start_logcat()` - Start client log streaming
- `stop_logcat()` - Stop client log streaming
- `subscribe()` - Get event receiver

#### 1.2 Add SteamVR Log Tailing

```rust
fn find_steamvr_log_path() -> Option<PathBuf> {
    // Windows: Check common Steam paths + registry
    // Linux: ~/.steam/steam/logs/vrserver.txt
}

async fn tail_steamvr_log(path: PathBuf, sender: broadcast::Sender<DiagEvent>) {
    // Use notify crate for file changes
    // Parse log lines and emit events
}
```

#### 1.3 Add ADB Status Polling

```rust
async fn poll_adb_status(sender: broadcast::Sender<DiagEvent>) {
    loop {
        let status = get_adb_status();
        sender.send(DiagEvent::AdbStatus(status));
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
```

#### 1.4 Add Client Logcat Streaming

```rust
async fn stream_logcat(adb_path: &str, device_serial: &str, sender: broadcast::Sender<DiagEvent>) {
    let mut child = Command::new(adb_path)
        .args(["-s", device_serial, "logcat", "-s", "alvr"])
        .stdout(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    for line in reader.lines() {
        sender.send(parse_logcat_line(line));
    }
}
```

### Phase 2: Web API Extensions

#### 2.1 Add Diagnostics Endpoints (`alvr/server_core/src/web_server.rs`)

```rust
// Add to router:
.nest(
    "/diagnostics",
    Router::new()
        .route("/events", routing::get(diagnostics_websocket))
        .route("/status", routing::get(get_diagnostics_status))
        .route("/logcat/start", routing::post(start_logcat))
        .route("/logcat/stop", routing::post(stop_logcat))
)

// New endpoint: Diagnostics WebSocket
async fn diagnostics_websocket(ws: WebSocketUpgrade, State(ctx): State<Arc<ConnectionContext>>) -> Response {
    // Stream DiagEvent objects
}

// New endpoint: Current status snapshot
async fn get_diagnostics_status(State(ctx): State<Arc<ConnectionContext>>) -> Json<DiagnosticsStatus> {
    // Return ADB status, SteamVR status, etc.
}
```

#### 2.2 Add Static File Serving for Web UI

```rust
// Serve static files from embedded assets or filesystem
.route("/diagnostics", routing::get(serve_diagnostics_html))
.route("/diagnostics/app.js", routing::get(serve_diagnostics_js))
```

### Phase 3: Web UI

#### 3.1 Create Diagnostics Web UI (`alvr/server_core/src/diagnostics_ui/`)

Single-page HTML/JS application:

```
diagnostics_ui/
├── index.html     # Main page
├── app.js         # WebSocket client + UI logic
└── style.css      # Styling
```

Features:
- Tab-based log viewer (Streamer | SteamVR | ADB | Client)
- Combined "All" view with source filtering
- Color-coded by log level
- Real-time updates via WebSocket
- ADB status panel showing:
  - Device connection state
  - Port forward status
  - Client package status
  - Start/Stop logcat button
- Auto-scroll with pause on hover
- Search/filter functionality
- Copy/export buttons

HTML Structure:
```html
<div id="app">
  <header>
    <h1>ALVR Diagnostics</h1>
    <div id="status-bar">
      <span id="device-status">No device</span>
      <span id="connection-status">Disconnected</span>
    </div>
  </header>

  <nav id="source-tabs">
    <button data-source="all" class="active">All</button>
    <button data-source="streamer">Streamer</button>
    <button data-source="steamvr">SteamVR</button>
    <button data-source="adb">ADB</button>
    <button data-source="client">Client</button>
  </nav>

  <aside id="adb-panel">
    <h3>ADB Status</h3>
    <div id="device-info">...</div>
    <button id="toggle-logcat">Start Logcat</button>
  </aside>

  <main id="log-container">
    <div id="log-entries"></div>
  </main>

  <footer>
    <input id="filter" placeholder="Filter...">
    <button id="copy-all">Copy All</button>
    <button id="clear">Clear</button>
  </footer>
</div>
```

### Phase 4: Dashboard Integration

#### 4.1 Add Diagnostics Tab (`alvr/dashboard/src/dashboard/components/diagnostics.rs`)

```rust
pub struct DiagnosticsTab {
    entries: VecDeque<DiagEntry>,
    selected_source: Option<DiagSource>,
    adb_status: Option<AdbStatus>,
    logcat_active: bool,
}

impl DiagnosticsTab {
    pub fn new() -> Self { ... }

    pub fn push_event(&mut self, event: DiagEvent) { ... }

    pub fn update_adb_status(&mut self, status: AdbStatus) { ... }

    pub fn ui(&mut self, ui: &mut Ui) -> Option<ServerRequest> {
        // Source filter buttons
        // ADB status panel
        // Log grid (similar to existing LogsTab)
        // Logcat start/stop button
    }
}
```

#### 4.2 Add to Dashboard Tabs (`alvr/dashboard/src/dashboard/mod.rs`)

```rust
enum Tab {
    // ... existing tabs ...
    Diagnostics,  // New
}

// Add ServerRequest variants:
pub enum ServerRequest {
    // ... existing ...
    StartLogcat,
    StopLogcat,
    GetDiagnosticsStatus,
}
```

#### 4.3 Update Data Sources (`alvr/dashboard/src/data_sources.rs`)

- Add diagnostics WebSocket subscription
- Handle DiagEvent messages
- Add REST calls for logcat control

### Phase 5: Event System Updates

#### 5.1 Extend EventType (`alvr/events/src/lib.rs`)

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum DiagSource {
    Streamer,
    SteamVR,
    Adb,
    Client,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DiagLogEntry {
    pub source: DiagSource,
    pub severity: LogSeverity,
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdbDeviceStatus {
    pub serial: String,
    pub state: String,
    pub ports_forwarded: Vec<u16>,
    pub client_package: Option<String>,
    pub client_running: bool,
}

// Add to EventType enum:
pub enum EventType {
    // ... existing ...
    DiagLog(DiagLogEntry),
    AdbDeviceStatus(AdbDeviceStatus),
    LogcatStateChanged { active: bool },
}
```

## File Changes Summary

### New Files
1. `alvr/server_core/src/diagnostics.rs` - Diagnostics service
2. `alvr/server_core/src/diagnostics_ui/index.html` - Web UI HTML
3. `alvr/server_core/src/diagnostics_ui/app.js` - Web UI JavaScript
4. `alvr/server_core/src/diagnostics_ui/style.css` - Web UI styles
5. `alvr/dashboard/src/dashboard/components/diagnostics.rs` - Dashboard tab

### Modified Files
1. `alvr/server_core/src/lib.rs` - Initialize DiagnosticsService
2. `alvr/server_core/src/web_server.rs` - Add diagnostics endpoints
3. `alvr/server_core/Cargo.toml` - Add `notify` dependency for file watching
4. `alvr/events/src/lib.rs` - Add DiagEvent types
5. `alvr/dashboard/src/dashboard/mod.rs` - Add Diagnostics tab
6. `alvr/dashboard/src/dashboard/components/mod.rs` - Export diagnostics module
7. `alvr/dashboard/src/data_sources.rs` - Handle diagnostics events/requests

## Dependencies

Add to `alvr/server_core/Cargo.toml`:
```toml
notify = "7"  # File system notifications for log tailing
```

## Testing Plan

1. Unit tests for log parsing functions
2. Integration test for ADB status polling
3. Manual testing:
   - Web UI in browser at `http://localhost:8082/diagnostics`
   - Dashboard tab functionality
   - Logcat streaming start/stop
   - Multi-source log interleaving
   - Filter functionality

## Future Enhancements

- Log persistence/export to file
- Log search with regex
- Timestamp range filtering
- Performance metrics in diagnostics view
- Remote diagnostics (connect to different ALVR instance)
- Automatic issue detection (pattern matching for common problems)
