use crate::dashboard::ServerRequest;
use alvr_common::LogSeverity;
use alvr_events::{AdbConnectionStatus, AdbDeviceStatus, DiagLogEntry, DiagSource};
use alvr_gui_common::theme::log_colors;
use eframe::egui::{self, Grid, RichText, ScrollArea, Ui};
use eframe::epaint::Color32;
use std::collections::VecDeque;

struct DiagEntry {
    color: Color32,
    timestamp: String,
    source: DiagSource,
    message: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SourceFilter {
    All,
    Streamer,
    SteamVR,
    Adb,
    Client,
}

impl SourceFilter {
    fn matches(&self, source: &DiagSource) -> bool {
        match self {
            SourceFilter::All => true,
            SourceFilter::Streamer => matches!(source, DiagSource::Streamer),
            SourceFilter::SteamVR => matches!(source, DiagSource::SteamVR),
            SourceFilter::Adb => matches!(source, DiagSource::Adb),
            SourceFilter::Client => matches!(source, DiagSource::Client),
        }
    }

    fn label(&self) -> &'static str {
        match self {
            SourceFilter::All => "All",
            SourceFilter::Streamer => "Streamer",
            SourceFilter::SteamVR => "SteamVR",
            SourceFilter::Adb => "ADB",
            SourceFilter::Client => "Client",
        }
    }
}

pub struct DiagnosticsTab {
    entries: VecDeque<DiagEntry>,
    log_limit: usize,
    source_filter: SourceFilter,
    adb_status: AdbConnectionStatus,
    logcat_active: bool,
}

impl DiagnosticsTab {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            log_limit: 2000,
            source_filter: SourceFilter::All,
            adb_status: AdbConnectionStatus::NotInstalled,
            logcat_active: false,
        }
    }

    pub fn push_diag_log(&mut self, timestamp: String, entry: DiagLogEntry) {
        let color = match entry.severity {
            LogSeverity::Error => log_colors::ERROR_LIGHT,
            LogSeverity::Warning => log_colors::WARNING_LIGHT,
            LogSeverity::Info => log_colors::INFO_LIGHT,
            LogSeverity::Debug => log_colors::DEBUG_LIGHT,
        };

        self.entries.push_back(DiagEntry {
            color,
            timestamp,
            source: entry.source,
            message: entry.content,
        });

        if self.entries.len() > self.log_limit {
            self.entries.pop_front();
        }
    }

    pub fn update_adb_status(&mut self, status: AdbConnectionStatus) {
        self.adb_status = status;
    }

    pub fn update_logcat_state(&mut self, active: bool) {
        self.logcat_active = active;
    }

    fn source_color(source: &DiagSource) -> Color32 {
        match source {
            DiagSource::Streamer => Color32::from_rgb(167, 139, 250), // Purple
            DiagSource::SteamVR => Color32::from_rgb(52, 211, 153),   // Green
            DiagSource::Adb => Color32::from_rgb(251, 191, 36),       // Yellow
            DiagSource::Client => Color32::from_rgb(244, 114, 182),   // Pink
        }
    }

    fn source_label(source: &DiagSource) -> &'static str {
        match source {
            DiagSource::Streamer => "Streamer",
            DiagSource::SteamVR => "SteamVR",
            DiagSource::Adb => "ADB",
            DiagSource::Client => "Client",
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, connected_to_server: bool) -> Option<ServerRequest> {
        let mut request = None;

        ui.horizontal(|ui| {
            // Source filter buttons
            ui.label("Filter: ");
            for filter in [
                SourceFilter::All,
                SourceFilter::Streamer,
                SourceFilter::SteamVR,
                SourceFilter::Adb,
                SourceFilter::Client,
            ] {
                if ui
                    .selectable_label(self.source_filter == filter, filter.label())
                    .clicked()
                {
                    self.source_filter = filter;
                }
            }

            ui.separator();

            if ui.button("Clear").clicked() {
                self.entries.clear();
            }

            if ui.button("Copy All").clicked() {
                let text: String = self
                    .entries
                    .iter()
                    .filter(|e| self.source_filter.matches(&e.source))
                    .map(|e| {
                        format!(
                            "{} [{}] {}\n",
                            e.timestamp,
                            Self::source_label(&e.source),
                            e.message
                        )
                    })
                    .collect();
                ui.output_mut(|out| {
                    out.commands
                        .push(egui::output::OutputCommand::CopyText(text));
                });
            }

            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.separator();

                if ui.button("Open Web UI").clicked() {
                    let _ = open::that("http://localhost:8082/diagnostics");
                }
            }
        });

        ui.add_space(10.0);

        // Two-column layout: ADB status panel on left, logs on right
        ui.horizontal(|ui| {
            // Left panel: ADB Status
            ui.vertical(|ui| {
                ui.set_min_width(200.0);
                ui.set_max_width(250.0);

                ui.heading("ADB Status");
                ui.add_space(5.0);

                match &self.adb_status {
                    AdbConnectionStatus::NotInstalled => {
                        ui.colored_label(log_colors::ERROR_LIGHT, "ADB Not Installed");
                    }
                    AdbConnectionStatus::NoDevices => {
                        ui.colored_label(log_colors::WARNING_LIGHT, "No Devices Connected");
                    }
                    AdbConnectionStatus::DeviceFound(device) => {
                        self.render_device_info(ui, device);
                    }
                }

                ui.add_space(10.0);

                // Logcat status (auto-managed)
                ui.heading("Client Logcat");
                ui.add_space(5.0);

                if !connected_to_server {
                    ui.colored_label(log_colors::WARNING_LIGHT, "Server disconnected");
                } else if !matches!(&self.adb_status, AdbConnectionStatus::DeviceFound(_)) {
                    ui.label("Waiting for device...");
                } else if self.logcat_active {
                    ui.colored_label(Color32::from_rgb(74, 222, 128), "Streaming...");
                } else {
                    ui.label("Starting...");
                }
            });

            ui.separator();

            // Right panel: Logs
            ui.vertical(|ui| {
                let filtered_count = self
                    .entries
                    .iter()
                    .filter(|e| self.source_filter.matches(&e.source))
                    .count();

                ui.label(format!(
                    "Showing {} / {} logs",
                    filtered_count,
                    self.entries.len()
                ));

                ScrollArea::both()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Grid::new("diagnostics_log_grid")
                            .spacing((10.0, 2.0))
                            .num_columns(3)
                            .striped(true)
                            .show(ui, |ui| {
                                for entry in &self.entries {
                                    if !self.source_filter.matches(&entry.source) {
                                        continue;
                                    }

                                    // Timestamp
                                    ui.colored_label(
                                        entry.color,
                                        RichText::new(&entry.timestamp).size(11.0),
                                    );

                                    // Source badge
                                    ui.colored_label(
                                        Self::source_color(&entry.source),
                                        RichText::new(Self::source_label(&entry.source))
                                            .size(11.0)
                                            .strong(),
                                    );

                                    // Message
                                    ui.colored_label(
                                        entry.color,
                                        RichText::new(&entry.message).size(11.0),
                                    );

                                    ui.end_row();
                                }
                            });
                    });
            });
        });

        request
    }

    fn render_device_info(&self, ui: &mut Ui, device: &AdbDeviceStatus) {
        ui.group(|ui| {
            ui.label(RichText::new("Device").strong());
            ui.label(&device.serial);

            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label("State:");
                let state_color = if device.state == "Device" {
                    Color32::from_rgb(74, 222, 128) // Green
                } else {
                    log_colors::WARNING_LIGHT
                };
                ui.colored_label(state_color, &device.state);
            });

            ui.add_space(5.0);

            ui.label(RichText::new("Port Forwards").strong());
            if device.ports_forwarded.is_empty() {
                ui.label("None");
            } else {
                ui.horizontal_wrapped(|ui| {
                    for port in &device.ports_forwarded {
                        ui.code(port.to_string());
                    }
                });
            }

            ui.add_space(5.0);

            ui.label(RichText::new("ALVR Client").strong());
            if let Some(ref pkg) = device.client_package {
                ui.label(pkg);
                ui.horizontal(|ui| {
                    ui.label("Running:");
                    if device.client_running {
                        ui.colored_label(Color32::from_rgb(74, 222, 128), "Yes");
                    } else {
                        ui.label("No");
                    }
                });
            } else {
                ui.colored_label(log_colors::WARNING_LIGHT, "Not installed");
            }
        });
    }
}
