//! Diagnostics service for aggregating logs from multiple sources
//!
//! This module provides real-time log streaming from:
//! - Streamer (server) logs (reused from existing EVENTS_SENDER)
//! - SteamVR logs (by tailing vrserver.txt)
//! - ADB device status (periodic polling)
//! - Client logs (via adb logcat)
//!
//! All logs are written to disk with unified timestamps in ISO 8601 format:
//! - Individual source files: steamvr.log, client.log, streamer.log
//! - Merged file: diagnostics.log (all sources combined, chronologically ordered)

use crate::{FILESYSTEM_LAYOUT, logging_backend::EVENTS_SENDER};
use alvr_common::{LogSeverity, info, warn, error, parking_lot::Mutex};
use alvr_events::Event;
use alvr_events::{
    AdbConnectionStatus, AdbDeviceStatus, DiagLogEntry, DiagSource, EventType,
};
use chrono::{DateTime, Datelike, Local, NaiveDateTime, TimeZone};
use notify::{RecursiveMode, Watcher, Event as NotifyEvent, EventKind};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Seek, SeekFrom, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::broadcast;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Event broadcast channel capacity
const DIAG_EVENT_CAPACITY: usize = 512;

/// ADB polling interval
const ADB_POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Max log lines to buffer when tailing files
#[allow(dead_code)]
const MAX_TAIL_LINES: usize = 100;

/// Max size for individual log files (10 MB)
const MAX_LOG_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Unified timestamp format (ISO 8601 with milliseconds)
const UNIFIED_TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3f";

/// Log file writer for writing normalized logs to disk
pub struct LogFileWriter {
    /// Path to the log directory
    log_dir: PathBuf,
    /// Individual source log files
    steamvr_file: Option<File>,
    client_file: Option<File>,
    streamer_file: Option<File>,
    /// Merged log file (all sources)
    merged_file: Option<File>,
}

impl LogFileWriter {
    /// Create a new log file writer
    pub fn new() -> Option<Self> {
        let layout = FILESYSTEM_LAYOUT.get()?;
        let log_dir = layout.log_dir.clone();

        // Create log directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&log_dir) {
            error!("Failed to create log directory: {}", e);
            return None;
        }

        let mut writer = Self {
            log_dir,
            steamvr_file: None,
            client_file: None,
            streamer_file: None,
            merged_file: None,
        };

        // Open all log files
        writer.steamvr_file = writer.open_log_file("steamvr.log");
        writer.client_file = writer.open_log_file("client.log");
        writer.streamer_file = writer.open_log_file("streamer.log");
        writer.merged_file = writer.open_log_file("diagnostics.log");

        Some(writer)
    }

    /// Open a log file for appending
    fn open_log_file(&self, filename: &str) -> Option<File> {
        let path = self.log_dir.join(filename);

        // Check if file exceeds max size and rotate if needed
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > MAX_LOG_FILE_SIZE {
                let backup_path = self.log_dir.join(format!("{}.old", filename));
                let _ = std::fs::rename(&path, backup_path);
            }
        }

        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| error!("Failed to open log file {}: {}", filename, e))
            .ok()
    }

    /// Write a log entry to the appropriate files
    pub fn write_entry(&mut self, entry: &UnifiedLogEntry) {
        let line = entry.format_line();

        // Write to source-specific file
        let source_file = match entry.source {
            DiagSource::SteamVR => &mut self.steamvr_file,
            DiagSource::Client => &mut self.client_file,
            DiagSource::Streamer => &mut self.streamer_file,
            DiagSource::Adb => &mut self.streamer_file, // ADB logs go with streamer
        };

        if let Some(file) = source_file {
            let _ = writeln!(file, "{}", line);
        }

        // Write to merged file
        if let Some(ref mut file) = self.merged_file {
            let _ = writeln!(file, "{}", line);
        }
    }

    /// Flush all log files
    pub fn flush(&mut self) {
        if let Some(ref mut f) = self.steamvr_file { let _ = f.flush(); }
        if let Some(ref mut f) = self.client_file { let _ = f.flush(); }
        if let Some(ref mut f) = self.streamer_file { let _ = f.flush(); }
        if let Some(ref mut f) = self.merged_file { let _ = f.flush(); }
    }
}

/// A log entry with unified timestamp
#[derive(Clone, Debug)]
pub struct UnifiedLogEntry {
    /// Timestamp in UTC
    pub timestamp: DateTime<Local>,
    /// Log source
    pub source: DiagSource,
    /// Severity level
    pub severity: LogSeverity,
    /// Log message content
    pub content: String,
}

impl UnifiedLogEntry {
    /// Format the log entry as a single line
    pub fn format_line(&self) -> String {
        format!(
            "{} [{}] [{}] {}",
            self.timestamp.format(UNIFIED_TIMESTAMP_FORMAT),
            self.source_tag(),
            self.severity_tag(),
            self.content
        )
    }

    fn source_tag(&self) -> &'static str {
        match self.source {
            DiagSource::SteamVR => "SteamVR",
            DiagSource::Client => "Client",
            DiagSource::Streamer => "Streamer",
            DiagSource::Adb => "ADB",
        }
    }

    fn severity_tag(&self) -> &'static str {
        match self.severity {
            LogSeverity::Error => "ERROR",
            LogSeverity::Warning => "WARN",
            LogSeverity::Info => "INFO",
            LogSeverity::Debug => "DEBUG",
        }
    }
}

/// Parse SteamVR timestamp format: "Fri Nov 28 2025 07:51:23.075"
fn parse_steamvr_timestamp(line: &str) -> Option<DateTime<Local>> {
    // Format: "Weekday Mon DD YYYY HH:MM:SS.mmm [Level]"
    // Example: "Fri Nov 28 2025 07:51:23.075 [Info]"

    // Find the timestamp portion (before the first '[')
    let ts_end = line.find('[')?;
    let ts_str = line[..ts_end].trim();

    // Parse: "Fri Nov 28 2025 07:51:23.075"
    let dt = NaiveDateTime::parse_from_str(ts_str, "%a %b %d %Y %H:%M:%S%.3f").ok()?;
    Some(Local.from_local_datetime(&dt).single()?)
}

/// Parse Android logcat timestamp format: "11-28 07:20:47.441"
fn parse_logcat_timestamp(line: &str) -> Option<DateTime<Local>> {
    // Format: "MM-DD HH:MM:SS.mmm PID TID Level Tag: Message"
    // Example: "11-28 07:20:47.441  1442  1442 V WorldManagerService:"

    // Extract the timestamp portion (first 18 chars: "MM-DD HH:MM:SS.mmm")
    if line.len() < 18 {
        return None;
    }

    let ts_str = &line[..18];

    // Parse month-day and time
    let parts: Vec<&str> = ts_str.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let date_part = parts[0]; // "MM-DD"
    let time_part = parts[1]; // "HH:MM:SS.mmm"

    let date_parts: Vec<&str> = date_part.split('-').collect();
    if date_parts.len() != 2 {
        return None;
    }

    let month: u32 = date_parts[0].parse().ok()?;
    let day: u32 = date_parts[1].parse().ok()?;

    // Use current year
    let year = Local::now().year();

    // Parse time
    let time_parts: Vec<&str> = time_part.split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }

    let hour: u32 = time_parts[0].parse().ok()?;
    let minute: u32 = time_parts[1].parse().ok()?;

    // Split seconds and milliseconds
    let sec_parts: Vec<&str> = time_parts[2].split('.').collect();
    let second: u32 = sec_parts[0].parse().ok()?;
    let millis: u32 = sec_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    Local
        .with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .map(|dt| dt + chrono::Duration::milliseconds(millis as i64))
}

/// Extract message content from SteamVR log line (after severity tag)
fn extract_steamvr_message(line: &str) -> String {
    // Find the end of the severity tag like "[Info] - "
    if let Some(bracket_end) = line.find("] - ") {
        line[bracket_end + 4..].to_string()
    } else if let Some(bracket_end) = line.find(']') {
        line[bracket_end + 1..].trim().to_string()
    } else {
        line.to_string()
    }
}

/// Extract message content from logcat line (after tag:)
fn extract_logcat_message(line: &str) -> String {
    // Format: "MM-DD HH:MM:SS.mmm  PID  TID Level Tag: Message"
    // Find the first colon after the tag
    if let Some(colon_pos) = line.find(": ") {
        // Skip past "MM-DD HH:MM:SS.mmm  PID  TID Level "
        line[colon_pos + 2..].to_string()
    } else {
        // Just skip the timestamp and return the rest
        if line.len() > 33 {
            line[33..].to_string()
        } else {
            line.to_string()
        }
    }
}

/// Diagnostics event sent to subscribers
#[derive(Clone, Debug)]
pub enum DiagEvent {
    Log(DiagLogEntry),
    AdbStatus(AdbConnectionStatus),
    LogcatState { active: bool },
}

impl From<DiagEvent> for EventType {
    fn from(event: DiagEvent) -> Self {
        match event {
            DiagEvent::Log(entry) => EventType::DiagLog(entry),
            DiagEvent::AdbStatus(status) => EventType::DiagAdbStatus(status),
            DiagEvent::LogcatState { active } => EventType::DiagLogcatState { active },
        }
    }
}

/// Max stored log entries
const MAX_STORED_LOGS: usize = 5000;

/// Shared state for the diagnostics service
pub struct DiagnosticsState {
    /// Path to ADB executable (if available)
    adb_path: Option<String>,
    /// Currently connected device serial
    current_device: Option<String>,
    /// Logcat child process
    logcat_process: Option<Child>,
    /// Whether logcat is running
    logcat_active: AtomicBool,
    /// Event broadcast sender
    events_sender: broadcast::Sender<DiagEvent>,
    /// Shutdown flag
    shutdown: AtomicBool,
    /// Stored log entries (ring buffer)
    stored_logs: std::collections::VecDeque<StoredLogEntry>,
    /// Last known ADB status
    last_adb_status: Option<AdbConnectionStatus>,
    /// Log file writer for disk output
    log_writer: Option<LogFileWriter>,
}

/// A stored log entry with timestamp
#[derive(Clone, Debug, serde::Serialize)]
pub struct StoredLogEntry {
    pub timestamp: String,
    pub source: DiagSource,
    pub severity: LogSeverity,
    pub content: String,
}

impl DiagnosticsState {
    pub fn new() -> (Arc<Mutex<Self>>, broadcast::Receiver<DiagEvent>) {
        let (sender, receiver) = broadcast::channel(DIAG_EVENT_CAPACITY);

        let state = Arc::new(Mutex::new(Self {
            adb_path: None,
            current_device: None,
            logcat_process: None,
            logcat_active: AtomicBool::new(false),
            events_sender: sender,
            shutdown: AtomicBool::new(false),
            stored_logs: std::collections::VecDeque::with_capacity(MAX_STORED_LOGS),
            last_adb_status: None,
            log_writer: LogFileWriter::new(),
        }));

        (state, receiver)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DiagEvent> {
        self.events_sender.subscribe()
    }

    pub fn send_event(&mut self, event: DiagEvent) {
        let timestamp = chrono::Local::now().format("%H:%M:%S%.3f").to_string();

        // Store event in buffer
        match &event {
            DiagEvent::AdbStatus(status) => {
                self.last_adb_status = Some(status.clone());
            }
            DiagEvent::Log(entry) => {
                // Store log entry
                self.stored_logs.push_back(StoredLogEntry {
                    timestamp: timestamp.clone(),
                    source: entry.source.clone(),
                    severity: entry.severity,
                    content: entry.content.clone(),
                });
                // Trim if over limit
                while self.stored_logs.len() > MAX_STORED_LOGS {
                    self.stored_logs.pop_front();
                }
            }
            DiagEvent::LogcatState { .. } => {}
        }

        // Send to diagnostics-specific channel
        let _ = self.events_sender.send(event.clone());

        // Also send to main events channel so dashboard receives it
        let event_type: EventType = event.into();
        let main_event = Event {
            timestamp,
            event_type,
        };
        let _ = EVENTS_SENDER.send(main_event);
    }

    /// Get stored logs
    pub fn get_stored_logs(&self) -> Vec<StoredLogEntry> {
        self.stored_logs.iter().cloned().collect()
    }

    /// Get last known ADB status
    pub fn get_last_adb_status(&self) -> Option<AdbConnectionStatus> {
        self.last_adb_status.clone()
    }

    pub fn is_logcat_active(&self) -> bool {
        self.logcat_active.load(Ordering::SeqCst)
    }

    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }

    /// Write a unified log entry to disk and broadcast
    pub fn write_unified_log(&mut self, entry: UnifiedLogEntry) {
        let timestamp_str = entry.timestamp.format("%H:%M:%S%.3f").to_string();

        // Write to disk files
        if let Some(ref mut writer) = self.log_writer {
            writer.write_entry(&entry);
        }

        // Store in memory buffer
        self.stored_logs.push_back(StoredLogEntry {
            timestamp: timestamp_str.clone(),
            source: entry.source.clone(),
            severity: entry.severity,
            content: entry.content.clone(),
        });

        // Trim if over limit
        while self.stored_logs.len() > MAX_STORED_LOGS {
            self.stored_logs.pop_front();
        }

        // Create DiagLogEntry for broadcasting
        let diag_entry = DiagLogEntry {
            source: entry.source,
            severity: entry.severity,
            content: entry.content,
        };

        // Send to diagnostics-specific channel
        let _ = self.events_sender.send(DiagEvent::Log(diag_entry.clone()));

        // Also send to main events channel so dashboard receives it
        let main_event = Event {
            timestamp: timestamp_str,
            event_type: EventType::DiagLog(diag_entry),
        };
        let _ = EVENTS_SENDER.send(main_event);
    }

    /// Flush log files to disk
    pub fn flush_logs(&mut self) {
        if let Some(ref mut writer) = self.log_writer {
            writer.flush();
        }
    }
}

/// Find SteamVR log file path
pub fn find_steamvr_log_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        // Try common Steam installation paths on Windows
        let possible_paths = [
            dirs::data_local_dir().map(|p| p.join("Steam/logs/vrserver.txt")),
            Some(PathBuf::from("C:/Program Files (x86)/Steam/logs/vrserver.txt")),
            Some(PathBuf::from("C:/Program Files/Steam/logs/vrserver.txt")),
            dirs::home_dir().map(|p| p.join("Steam/logs/vrserver.txt")),
        ];

        for path_opt in possible_paths.iter().flatten() {
            if path_opt.exists() {
                return Some(path_opt.clone());
            }
        }

        // Try to find via registry (simplified - would need winreg crate for full impl)
        None
    }

    #[cfg(target_os = "linux")]
    {
        let possible_paths = [
            dirs::home_dir().map(|p| p.join(".steam/steam/logs/vrserver.txt")),
            dirs::home_dir().map(|p| p.join(".local/share/Steam/logs/vrserver.txt")),
        ];

        for path_opt in possible_paths.iter().flatten() {
            if path_opt.exists() {
                return Some(path_opt.clone());
            }
        }

        None
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .map(|p| p.join("Library/Application Support/Steam/logs/vrserver.txt"))
            .filter(|p| p.exists())
    }
}

/// Parse a SteamVR log line into a UnifiedLogEntry with normalized timestamp
fn parse_steamvr_log_line(line: &str) -> Option<UnifiedLogEntry> {
    // SteamVR log format: "Fri Nov 28 2025 07:51:23.075 [Info] - Message"
    if line.trim().is_empty() {
        return None;
    }

    // Try to parse the timestamp, fall back to current time if parsing fails
    let timestamp = parse_steamvr_timestamp(line).unwrap_or_else(Local::now);

    // Determine severity from the [Level] tag
    let severity = if line.contains("[Error]") {
        LogSeverity::Error
    } else if line.contains("[Warning]") {
        LogSeverity::Warning
    } else if line.contains("[Debug]") {
        LogSeverity::Debug
    } else {
        LogSeverity::Info
    };

    // Extract the message content (after the severity tag)
    let content = extract_steamvr_message(line);

    Some(UnifiedLogEntry {
        timestamp,
        source: DiagSource::SteamVR,
        severity,
        content,
    })
}

/// Parse an Android logcat line into a UnifiedLogEntry with normalized timestamp
fn parse_logcat_line(line: &str) -> Option<UnifiedLogEntry> {
    // Logcat format: "MM-DD HH:MM:SS.mmm PID TID LEVEL TAG: Message"
    // Or threadtime format: "MM-DD HH:MM:SS.mmm PID TID Level Tag: Message"
    if line.trim().is_empty() {
        return None;
    }

    // Try to parse the timestamp, fall back to current time if parsing fails
    let timestamp = parse_logcat_timestamp(line).unwrap_or_else(Local::now);

    // Look for severity indicator (single letter after timestamp)
    let severity = if line.contains(" E ") || line.contains("/E ") || line.contains(" E/") {
        LogSeverity::Error
    } else if line.contains(" W ") || line.contains("/W ") || line.contains(" W/") {
        LogSeverity::Warning
    } else if line.contains(" D ") || line.contains("/D ") || line.contains(" D/") {
        LogSeverity::Debug
    } else {
        LogSeverity::Info
    };

    // Extract the message content
    let content = extract_logcat_message(line);

    Some(UnifiedLogEntry {
        timestamp,
        source: DiagSource::Client,
        severity,
        content,
    })
}

/// Start tailing SteamVR logs
pub async fn start_steamvr_log_tail(
    state: Arc<Mutex<DiagnosticsState>>,
) {
    let log_path = match find_steamvr_log_path() {
        Some(path) => {
            info!("Found SteamVR log at: {:?}", path);
            path
        }
        None => {
            warn!("SteamVR log file not found, skipping log tailing");
            return;
        }
    };

    // Get initial file position (end of file)
    let mut last_pos = match std::fs::metadata(&log_path) {
        Ok(meta) => meta.len(),
        Err(_) => 0,
    };

    // Set up file watcher
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    let watcher_result = notify::recommended_watcher(move |res: Result<NotifyEvent, _>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_)) {
                let _ = tx.blocking_send(());
            }
        }
    });

    let mut watcher = match watcher_result {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create file watcher: {}", e);
            return;
        }
    };

    if let Err(e) = watcher.watch(&log_path, RecursiveMode::NonRecursive) {
        error!("Failed to watch SteamVR log: {}", e);
        return;
    }

    // Read new lines when file changes
    loop {
        tokio::select! {
            _ = rx.recv() => {
                // File changed, read new content
                if let Ok(mut file) = File::open(&log_path) {
                    if file.seek(SeekFrom::Start(last_pos)).is_ok() {
                        let reader = BufReader::new(&mut file);
                        for line in reader.lines().map_while(Result::ok) {
                            if let Some(entry) = parse_steamvr_log_line(&line) {
                                state.lock().write_unified_log(entry);
                            }
                        }
                        if let Ok(pos) = file.stream_position() {
                            last_pos = pos;
                        }
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                // Check for shutdown
                if state.lock().is_shutdown_requested() {
                    break;
                }
            }
        }
    }

    drop(watcher);
}

/// Get current ADB status
pub fn get_adb_status(adb_path: &str) -> AdbConnectionStatus {
    // List devices
    let devices = match alvr_adb::commands::list_devices(adb_path) {
        Ok(d) => d,
        Err(_) => return AdbConnectionStatus::NoDevices,
    };

    // Find first non-loopback device
    let device = devices.into_iter().find(|d| {
        d.serial
            .as_ref()
            .map(|s| !s.starts_with("127.0.0.1"))
            .unwrap_or(false)
    });

    let Some(device) = device else {
        return AdbConnectionStatus::NoDevices;
    };

    let Some(serial) = device.serial else {
        return AdbConnectionStatus::NoDevices;
    };

    let state = device.connection_state
        .map(|s| format!("{:?}", s))
        .unwrap_or_else(|| "unknown".to_string());

    // Get forwarded ports
    let ports_forwarded = alvr_adb::commands::list_forwarded_ports(adb_path, &serial)
        .map(|fps| fps.into_iter().map(|fp| fp.local).collect())
        .unwrap_or_default();

    // Check for ALVR client packages
    let client_package = check_alvr_client_installed(adb_path, &serial);

    // Check if client is running
    let client_running = client_package.as_ref().map_or(false, |pkg| {
        alvr_adb::commands::get_process_id(adb_path, &serial, pkg)
            .map(|pid| pid.is_some())
            .unwrap_or(false)
    });

    AdbConnectionStatus::DeviceFound(AdbDeviceStatus {
        serial,
        state,
        ports_forwarded,
        client_package,
        client_running,
    })
}

/// Check which ALVR client package is installed
fn check_alvr_client_installed(adb_path: &str, device_serial: &str) -> Option<String> {
    let packages = [
        alvr_system_info::PACKAGE_NAME_STORE,
        alvr_system_info::PACKAGE_NAME_GITHUB_STABLE,
        alvr_system_info::PACKAGE_NAME_GITHUB_DEV,
    ];

    for pkg in packages {
        if alvr_adb::commands::is_package_installed(adb_path, device_serial, pkg).unwrap_or(false) {
            return Some(pkg.to_string());
        }
    }

    None
}

/// Poll ADB status periodically
pub async fn start_adb_status_polling(
    state: Arc<Mutex<DiagnosticsState>>,
) {
    // Try to get ADB path
    let layout = match FILESYSTEM_LAYOUT.get() {
        Some(l) => l,
        None => {
            warn!("Filesystem layout not initialized, ADB polling disabled");
            return;
        }
    };

    let adb_path = match alvr_adb::commands::get_adb_path(layout) {
        Some(p) => p,
        None => {
            info!("ADB not found, status polling disabled");
            state.lock().send_event(DiagEvent::AdbStatus(AdbConnectionStatus::NotInstalled));
            return;
        }
    };

    // Store ADB path
    state.lock().adb_path = Some(adb_path.clone());

    let mut previous_device: Option<String> = None;

    loop {
        let status = get_adb_status(&adb_path);

        // Update current device and auto-manage logcat
        match &status {
            AdbConnectionStatus::DeviceFound(dev) => {
                let device_changed = previous_device.as_ref() != Some(&dev.serial);

                {
                    let mut state_guard = state.lock();
                    state_guard.current_device = Some(dev.serial.clone());
                }

                // Auto-start logcat when device connects (or changes)
                let logcat_active = state.lock().is_logcat_active();
                if device_changed {
                    info!("[Diag] Device changed to {}, will start logcat", dev.serial);
                }
                if !logcat_active {
                    info!("[Diag] Logcat not active, attempting to start...");
                    match start_logcat(&state) {
                        Ok(()) => info!("[Diag] Logcat auto-started successfully"),
                        Err(e) => {
                            if !e.contains("already running") {
                                error!("[Diag] Auto-start logcat failed: {}", e);
                            }
                        }
                    }
                }

                previous_device = Some(dev.serial.clone());
            }
            _ => {
                // Device disconnected - stop logcat
                if previous_device.is_some() {
                    stop_logcat(&state);
                }
                state.lock().current_device = None;
                previous_device = None;
            }
        }

        state.lock().send_event(DiagEvent::AdbStatus(status));

        tokio::time::sleep(ADB_POLL_INTERVAL).await;

        if state.lock().is_shutdown_requested() {
            break;
        }
    }
}

/// Start streaming logcat from connected device
pub fn start_logcat(state: &Arc<Mutex<DiagnosticsState>>) -> Result<(), String> {
    let mut state_guard = state.lock();

    if state_guard.logcat_active.load(Ordering::SeqCst) {
        return Err("Logcat already running".to_string());
    }

    let adb_path = state_guard.adb_path.clone()
        .ok_or("ADB not available")?;

    let device_serial = state_guard.current_device.clone()
        .ok_or("No device connected")?;

    info!("[Diag] Starting logcat for device: {}", device_serial);

    // Build command - return all logs, no filtering
    let mut cmd = Command::new(&adb_path);
    cmd.args([
        "-s", &device_serial,
        "logcat",
        "-v", "threadtime",
        "-T", "100",  // Start with last 100 lines then stream new ones
    ]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn logcat: {}", e))?;

    let stdout = child.stdout.take()
        .ok_or("Failed to get logcat stdout")?;

    state_guard.logcat_process = Some(child);
    state_guard.logcat_active.store(true, Ordering::SeqCst);

    // Send state change event
    state_guard.send_event(DiagEvent::LogcatState { active: true });

    info!("[Diag] Logcat started, reading output...");

    // Spawn reader thread
    let state_clone = Arc::clone(state);
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut line_count = 0;

        for line in reader.lines().map_while(Result::ok) {
            line_count += 1;
            if line_count <= 3 {
                info!("[Diag] Logcat line {}: {}", line_count, &line[..line.len().min(100)]);
            }

            if let Some(entry) = parse_logcat_line(&line) {
                state_clone.lock().write_unified_log(entry);
            }

            // Check if we should stop
            if !state_clone.lock().logcat_active.load(Ordering::SeqCst) {
                break;
            }
        }

        info!("[Diag] Logcat reader stopped after {} lines", line_count);

        // Clean up
        let mut state_guard = state_clone.lock();
        state_guard.logcat_active.store(false, Ordering::SeqCst);
        state_guard.send_event(DiagEvent::LogcatState { active: false });
    });

    Ok(())
}

/// Stop logcat streaming
pub fn stop_logcat(state: &Arc<Mutex<DiagnosticsState>>) {
    let mut state_guard = state.lock();

    state_guard.logcat_active.store(false, Ordering::SeqCst);

    if let Some(mut child) = state_guard.logcat_process.take() {
        let _ = child.kill();
        let _ = child.wait();
    }

    state_guard.send_event(DiagEvent::LogcatState { active: false });
}

/// Get a snapshot of current diagnostics status
pub fn get_diagnostics_snapshot(state: &Arc<Mutex<DiagnosticsState>>) -> DiagnosticsSnapshot {
    let state_guard = state.lock();

    let adb_status = state_guard.last_adb_status.clone().unwrap_or_else(|| {
        if let Some(ref adb_path) = state_guard.adb_path {
            get_adb_status(adb_path)
        } else {
            AdbConnectionStatus::NotInstalled
        }
    });

    DiagnosticsSnapshot {
        steamvr_log_path: find_steamvr_log_path(),
        adb_status,
        logcat_active: state_guard.logcat_active.load(Ordering::SeqCst),
        log_count: state_guard.stored_logs.len(),
    }
}

/// Get full diagnostics data including stored logs
pub fn get_diagnostics_full(state: &Arc<Mutex<DiagnosticsState>>) -> DiagnosticsFull {
    let state_guard = state.lock();

    let adb_status = state_guard.last_adb_status.clone().unwrap_or_else(|| {
        if let Some(ref adb_path) = state_guard.adb_path {
            get_adb_status(adb_path)
        } else {
            AdbConnectionStatus::NotInstalled
        }
    });

    DiagnosticsFull {
        steamvr_log_path: find_steamvr_log_path(),
        adb_status,
        logcat_active: state_guard.logcat_active.load(Ordering::SeqCst),
        logs: state_guard.stored_logs.iter().cloned().collect(),
    }
}

#[derive(serde::Serialize)]
pub struct DiagnosticsSnapshot {
    pub steamvr_log_path: Option<PathBuf>,
    pub adb_status: AdbConnectionStatus,
    pub logcat_active: bool,
    pub log_count: usize,
}

#[derive(serde::Serialize)]
pub struct DiagnosticsFull {
    pub steamvr_log_path: Option<PathBuf>,
    pub adb_status: AdbConnectionStatus,
    pub logcat_active: bool,
    pub logs: Vec<StoredLogEntry>,
}
