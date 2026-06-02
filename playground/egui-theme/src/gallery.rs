//! Central preview — a telemetry instrument. Everything left-locked; the readout
//! row (label · hairline track · right-locked mono value + min/max ticks) is the
//! motif, repeated across sliders, stats and groups.

use crate::params::ThemeParams;
use crate::typography::{self, ts_display, ts_eyebrow};
use alvr_gui_common::{DisplayString, button_group_clicked, switch, theme};
use egui::{
    Align, Button, Color32, ComboBox, CornerRadius, FontFamily, Frame, Grid, Layout, Margin, Rect,
    RichText, Sense, Slider, Stroke, Ui, Vec2,
};
use std::ops::RangeInclusive;

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
            bitrate: 120.0,
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

struct SliderSpec {
    label: &'static str,
    unit: &'static str,
    range: RangeInclusive<f32>,
    step: f32,
    decimals: usize,
}

pub fn show(ui: &mut Ui, s: &mut DemoState, p: &ThemeParams) {
    header(ui, p);
    stats_card(ui, p);
    connection_card(ui, s, p);
    video_card(ui, s, p);
    actions_card(ui, p);
    logs_card(ui, p);
    palette_card(ui, p);
}

// ---- header -----------------------------------------------------------------

fn header(ui: &mut Ui, p: &ThemeParams) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("ALVR").text_style(ts_display()).color(p.fg));
        ui.add_space(8.0);
        ui.label(mono("v21.0·dev", 11.0, p.text_faint));
    });
    ui.label(RichText::new("Air Light VR — streamer dashboard").color(p.text_muted));
    ui.add_space(12.0);
    accent_rule(ui, p);
    ui.add_space(14.0);
}

fn accent_rule(ui: &mut Ui, p: &ThemeParams) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 2.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(1), p.separator);
    let lead = Rect::from_min_size(rect.min, Vec2::new(52.0, 2.0));
    ui.painter().rect_filled(lead, CornerRadius::same(1), p.accent);
}

// ---- cards ------------------------------------------------------------------

fn stats_card(ui: &mut Ui, p: &ThemeParams) {
    card(ui, p, "Live statistics", |ui| {
        ui.columns(4, |c| {
            stat_tile(&mut c[0], p, "Total latency", "34", "ms", p.accent_hover);
            stat_tile(&mut c[1], p, "Encode", "5.1", "ms", p.ok_green);
            stat_tile(&mut c[2], p, "Network", "11", "ms", p.warning);
            stat_tile(&mut c[3], p, "Client FPS", "90", "", p.fg);
        });
    });
}

fn connection_card(ui: &mut Ui, s: &mut DemoState, p: &ThemeParams) {
    card(ui, p, "Connection", |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(s.name.as_str()).text_style(ts_display()).size(16.0).color(p.fg));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| status_pill(ui, p, s.enabled));
        });
        ui.add_space(8.0);
        switch_row(ui, p, &mut s.enabled, "Trust & auto-connect");
    });
}

fn video_card(ui: &mut Ui, s: &mut DemoState, p: &ThemeParams) {
    card(ui, p, "Video", |ui| {
        subgroup(ui, p, "Throughput", |ui| {
            slider_block(ui, p, &SliderSpec { label: "Bitrate", unit: "Mb/s", range: 10.0..=500.0, step: 5.0, decimals: 0 }, &mut s.bitrate);
            slider_block(ui, p, &SliderSpec { label: "Resolution scale", unit: "×", range: 0.5..=2.0, step: 0.05, decimals: 2 }, &mut s.resolution);
        });
        subgroup(ui, p, "Encoding", |ui| {
            switch_row(ui, p, &mut s.foveation, "Foveated encoding");
            ui.add_space(10.0);
            field_label(ui, p, "Codec");
            ui.add_space(4.0);
            ui.horizontal(|ui| button_group_clicked(ui, &s.codec_options, &mut s.codec));
            ui.add_space(10.0);
            field_label(ui, p, "Latency preset");
            ui.add_space(4.0);
            ComboBox::from_id_salt("preset")
                .selected_text(&s.preset)
                .show_ui(ui, |ui| {
                    for opt in ["Latency", "Balanced", "Quality"] {
                        ui.selectable_value(&mut s.preset, opt.to_owned(), opt);
                    }
                });
        });
    });
}

fn actions_card(ui: &mut Ui, p: &ThemeParams) {
    card(ui, p, "Actions", |ui| {
        ui.horizontal(|ui| {
            ui.add(Button::new(RichText::new("Save").color(p.on_accent)).fill(p.accent));
            ui.add(Button::new(RichText::new("Restart SteamVR").color(p.fg)));
            ui.add(Button::new(RichText::new("Cancel").color(p.text_muted)).frame(false));
        });
        ui.add_space(10.0);
        ui.hyperlink_to(RichText::new("Open the ALVR wiki ↗").size(13.0), "https://github.com/alvr-org/ALVR/wiki");
    });
}

fn logs_card(ui: &mut Ui, p: &ThemeParams) {
    let rows = [
        ("ERROR", p.log_error, "encoder pipeline stalled for 2 frames"),
        ("WARN", p.log_warning, "bitrate capped by network headroom"),
        ("INFO", p.log_info, "client connected · 192.168.1.42"),
        ("DEBUG", p.log_debug, "frame 12944 submitted in 4.1ms"),
        ("EVENT", p.log_event, "tracking pose received"),
    ];
    card(ui, p, "Logs", |ui| {
        Grid::new("logs")
            .num_columns(2)
            .min_row_height(20.0)
            .min_col_width(58.0)
            .spacing([16.0, 3.0])
            .show(ui, |ui| {
                for (level, color, msg) in rows {
                    ui.label(mono_strong(level, 11.0, color));
                    ui.label(mono(msg, 12.5, p.text_muted));
                    ui.end_row();
                }
            });
    });
}

fn palette_card(ui: &mut Ui, p: &ThemeParams) {
    use theme::graph_colors as g;
    let items = [
        (g::RENDER, "Render"),
        (g::IDLE, "Idle"),
        (g::TRANSCODE, "Transcode"),
        (g::NETWORK, "Network"),
        (g::SERVER_FPS, "Server FPS"),
        (g::CLIENT_FPS, "Client FPS"),
    ];
    card(ui, p, "Statistics graph palette", |ui| {
        Grid::new("palette")
            .num_columns(3)
            .min_row_height(22.0)
            .min_col_width(150.0)
            .spacing([20.0, 12.0])
            .show(ui, |ui| {
                for (i, (color, name)) in items.iter().enumerate() {
                    legend_dot(ui, p, *color, name);
                    if i % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
    });
}

fn legend_dot(ui: &mut Ui, p: &ThemeParams, color: Color32, name: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(3), color);
        ui.add_space(7.0);
        ui.label(RichText::new(name).size(12.5).color(p.text_muted));
    });
}

// ---- structure --------------------------------------------------------------

fn card(ui: &mut Ui, p: &ThemeParams, eyebrow_text: &str, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(p.section_bg)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(p.card_radius)
        .inner_margin(p.frame_padding)
        .shadow(p.shadow())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            eyebrow(ui, p, eyebrow_text);
            ui.add_space(p.frame_text_spacing + 6.0);
            add(ui);
        });
    ui.add_space(p.item_spacing.y + 4.0);
}

/// A labelled subgroup inside a card: eyebrow + hairline rule + indented content.
fn subgroup(ui: &mut Ui, p: &ThemeParams, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(spaced_upper(title)).text_style(ts_eyebrow()).color(p.text_muted));
        ui.add_space(10.0);
        let w = ui.available_width();
        if w > 8.0 {
            let (r, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
            ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, p.separator));
        }
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.vertical(|ui| add(ui));
    });
    ui.add_space(12.0);
}

// ---- readout rows -----------------------------------------------------------

const READOUT_MEASURE: f32 = 460.0;

fn slider_block(ui: &mut Ui, p: &ThemeParams, spec: &SliderSpec, value: &mut f32) {
    ui.scope(|ui| {
        ui.set_max_width(READOUT_MEASURE);
        ui.horizontal(|ui| {
            ui.label(RichText::new(spec.label).color(p.text_muted).size(13.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(mono(spec.unit, 11.0, p.text_faint));
                ui.add_space(3.0);
                ui.label(mono_strong(fmt(*value, spec.decimals), 15.0, p.accent_hover));
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(mono(fmt(*spec.range.start(), spec.decimals), 10.0, p.text_faint));
            let track = (ui.available_width() - 52.0).clamp(120.0, READOUT_MEASURE);
            ui.spacing_mut().slider_width = track;
            ui.add(Slider::new(value, spec.range.clone()).show_value(false).step_by(spec.step as f64));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(mono(fmt(*spec.range.end(), spec.decimals), 10.0, p.text_faint));
            });
        });
    });
    ui.add_space(10.0);
}

fn switch_row(ui: &mut Ui, p: &ThemeParams, value: &mut bool, label: &str) {
    ui.horizontal(|ui| {
        switch(ui, value);
        ui.add_space(6.0);
        ui.label(RichText::new(label).color(p.fg).size(13.0));
    });
}

// ---- atoms ------------------------------------------------------------------

fn eyebrow(ui: &mut Ui, p: &ThemeParams, text: &str) {
    ui.label(RichText::new(spaced_upper(text)).text_style(ts_eyebrow()).color(p.text_faint));
}

fn field_label(ui: &mut Ui, p: &ThemeParams, text: &str) {
    ui.label(RichText::new(text).color(p.text_muted).size(12.5));
}

fn stat_tile(ui: &mut Ui, p: &ThemeParams, label: &str, value: &str, unit: &str, accent: Color32) {
    Frame::new()
        .fill(p.darker_bg)
        .stroke(Stroke::new(1.0_f32, p.separator))
        .corner_radius(p.widget_radius)
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.horizontal(|ui| {
                    ui.label(mono_strong(value, 23.0, accent));
                    if !unit.is_empty() {
                        ui.label(mono(unit, 11.0, p.text_faint));
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new(spaced_upper(label)).text_style(ts_eyebrow()).color(p.text_muted));
            });
        });
}

fn status_pill(ui: &mut Ui, p: &ThemeParams, on: bool) {
    let (fg, text) = if on { (p.ok_green, "STREAMING") } else { (p.ko_red, "OFFLINE") };
    Frame::new()
        .fill(with_alpha(fg, 32))
        .corner_radius(CornerRadius::same(99))
        .inner_margin(Margin::symmetric(11, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 3.5, fg);
                ui.add_space(2.0);
                ui.label(RichText::new(spaced_upper(text)).text_style(ts_eyebrow()).color(fg));
            });
        });
}

// ---- text helpers -----------------------------------------------------------

fn mono(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text).family(FontFamily::Monospace).size(size).color(color)
}

fn mono_strong(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text)
        .family(FontFamily::Name(typography::MONO_STRONG.into()))
        .size(size)
        .color(color)
}

fn fmt(value: f32, decimals: usize) -> String {
    format!("{value:.decimals$}")
}

/// Fake letter-spacing for eyebrow runs (egui has no global tracking).
fn spaced_upper(text: &str) -> String {
    text.to_uppercase()
        .chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join("\u{200a}")
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}
