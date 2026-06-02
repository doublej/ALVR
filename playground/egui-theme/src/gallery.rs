//! Central panel: a representative slice of the ALVR dashboard, drawn with the
//! live theme so every edit is visible in context.

use crate::params::ThemeParams;
use alvr_gui_common::{DisplayString, button_group_clicked, switch, theme};
use egui::{ComboBox, CornerRadius, RichText, Sense, Ui};

pub struct DemoState {
    pub name: String,
    pub bitrate: f32,
    pub resolution: f32,
    pub enabled: bool,
    pub foveation: bool,
    pub codec: String,
    pub codec_options: Vec<DisplayString>,
    pub preset: String,
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            name: "Quest 3".to_owned(),
            bitrate: 100.0,
            resolution: 1.0,
            enabled: true,
            foveation: true,
            codec: "hevc".to_owned(),
            codec_options: vec![
                ("h264".to_owned(), "H.264".to_owned()).into(),
                ("hevc".to_owned(), "HEVC".to_owned()).into(),
                ("av1".to_owned(), "AV1".to_owned()).into(),
            ],
            preset: "Balanced".to_owned(),
        }
    }
}

pub fn show(ui: &mut Ui, s: &mut DemoState, p: &ThemeParams) {
    ui.heading("ALVR widget gallery");
    ui.label("Live preview — tweak on the left, copy paste-ready code on the right.");
    ui.add_space(p.frame_text_spacing);

    connection_section(ui, s, p);
    video_section(ui, s, p);
    controls_section(ui, s, p);
    logs_section(ui, p);
    graph_legend(ui, p);
}

fn connection_section(ui: &mut Ui, s: &mut DemoState, p: &ThemeParams) {
    section(ui, p, "Connection", |ui| {
        ui.horizontal(|ui| {
            ui.label("Headset:");
            ui.text_edit_singleline(&mut s.name);
        });
        ui.horizontal(|ui| {
            switch(ui, &mut s.enabled);
            ui.label("Trust device");
            let (color, text) = if s.enabled {
                (p.ok_green, "● Streaming")
            } else {
                (p.ko_red, "● Disconnected")
            };
            ui.colored_label(color, text);
        });
    });
}

fn video_section(ui: &mut Ui, s: &mut DemoState, p: &ThemeParams) {
    section(ui, p, "Video", |ui| {
        ui.add(egui::Slider::new(&mut s.bitrate, 10.0..=500.0).text("Bitrate (Mbps)"));
        ui.add(egui::Slider::new(&mut s.resolution, 0.5..=2.0).text("Resolution scale"));
        ui.horizontal(|ui| {
            switch(ui, &mut s.foveation);
            ui.label("Foveated encoding");
        });
        ui.horizontal(|ui| {
            ui.label("Codec:");
            button_group_clicked(ui, &s.codec_options, &mut s.codec);
        });
        ComboBox::from_label("Preset")
            .selected_text(&s.preset)
            .show_ui(ui, |ui| {
                for opt in ["Latency", "Balanced", "Quality"] {
                    ui.selectable_value(&mut s.preset, opt.to_owned(), opt);
                }
            });
    });
}

fn controls_section(ui: &mut Ui, _s: &mut DemoState, p: &ThemeParams) {
    section(ui, p, "Buttons & rules", |ui| {
        ui.horizontal(|ui| {
            let _ = ui.button("Save").clicked();
            let _ = ui.button("Restart SteamVR").clicked();
            ui.add_enabled(false, egui::Button::new("Disabled"));
        });
        ui.separator(); // a horizontal "rule"
        ui.hyperlink_to("ALVR wiki", "https://github.com/alvr-org/ALVR/wiki");
    });
}

fn logs_section(ui: &mut Ui, p: &ThemeParams) {
    section(ui, p, "Logs", |ui| {
        ui.colored_label(p.log_error, "[ERROR] encoder pipeline stalled");
        ui.colored_label(p.log_warning, "[WARN]  bitrate capped by network");
        ui.colored_label(p.log_info, "[INFO]  client connected: 192.168.1.42");
        ui.colored_label(p.log_debug, "[DEBUG] frame 12944 submitted in 4.1ms");
        ui.colored_label(p.log_event, "[EVENT] tracking pose received");
    });
}

fn graph_legend(ui: &mut Ui, p: &ThemeParams) {
    use theme::graph_colors as g;
    section(ui, p, "Statistics graph palette (compile-time)", |ui| {
        for (color, name) in [
            (g::RENDER, "Render"),
            (g::IDLE, "Idle"),
            (g::TRANSCODE, "Transcode"),
            (g::NETWORK, "Network"),
            (g::SERVER_FPS, "Server FPS"),
            (g::CLIENT_FPS, "Client FPS"),
        ] {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::hover());
                ui.painter().rect_filled(rect, CornerRadius::same(3), color);
                ui.label(name);
            });
        }
    });
}

/// A themed `Frame::group` matching the dashboard's section styling, driven by `p`.
fn section(ui: &mut Ui, p: &ThemeParams, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.add_space(p.frame_text_spacing);
    egui::Frame::group(ui.style())
        .fill(p.section_bg)
        .inner_margin(p.frame_padding)
        .corner_radius(p.corner_radius)
        .show(ui, |ui| {
            ui.label(RichText::new(title).strong());
            ui.add_space(p.frame_text_spacing);
            add(ui);
        });
}
