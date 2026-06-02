//! Left panel: live controls bound to `ThemeParams`.

use crate::params::ThemeParams;
use egui::{Color32, Ui, Vec2};
use std::ops::RangeInclusive;

pub fn show(ui: &mut Ui, p: &mut ThemeParams) {
    ui.heading("Style editor");
    if ui.button("⟲ Reset to ALVR defaults").clicked() {
        *p = ThemeParams::default();
    }
    ui.separator();

    ui.collapsing("Colors", |ui| {
        color(ui, "Accent", &mut p.accent);
        color(ui, "Background", &mut p.bg);
        color(ui, "Lighter bg", &mut p.lighter_bg);
        color(ui, "Section bg", &mut p.section_bg);
        color(ui, "Darker bg", &mut p.darker_bg);
        color(ui, "Separator", &mut p.separator_bg);
        color(ui, "Foreground", &mut p.fg);
        color(ui, "OK green", &mut p.ok_green);
        color(ui, "KO red", &mut p.ko_red);
    });

    ui.collapsing("Log colors", |ui| {
        color(ui, "Error", &mut p.log_error);
        color(ui, "Warning", &mut p.log_warning);
        color(ui, "Info", &mut p.log_info);
        color(ui, "Debug", &mut p.log_debug);
        color(ui, "Event", &mut p.log_event);
    });

    ui.collapsing("Spacing & shape", |ui| {
        scalar(ui, "Frame padding", &mut p.frame_padding, 0.0..=40.0);
        u8_scalar(ui, "Corner radius", &mut p.corner_radius, 0..=40);
        scalar(ui, "Frame text spacing", &mut p.frame_text_spacing, 0.0..=30.0);
        scalar(ui, "Body text size", &mut p.body_text_size, 8.0..=28.0);
        scalar(ui, "Slider width", &mut p.slider_width, 60.0..=400.0);
        scalar(ui, "Interact size", &mut p.interact_size, 16.0..=60.0);
        scalar(ui, "Scroll/switch dot", &mut p.scrollbar_dot_diameter, 8.0..=40.0);
        vec2(ui, "Item spacing", &mut p.item_spacing);
        vec2(ui, "Button padding", &mut p.button_padding);
        scalar(ui, "Tooltip delay (s)", &mut p.tooltip_delay, 0.0..=1.0);
    });
}

fn color(ui: &mut Ui, label: &str, value: &mut Color32) {
    ui.horizontal(|ui| {
        ui.color_edit_button_srgba(value);
        ui.label(label);
    });
}

fn scalar(ui: &mut Ui, label: &str, value: &mut f32, range: RangeInclusive<f32>) {
    ui.add(egui::Slider::new(value, range).text(label));
}

fn u8_scalar(ui: &mut Ui, label: &str, value: &mut u8, range: RangeInclusive<u8>) {
    ui.add(egui::Slider::new(value, range).text(label));
}

fn vec2(ui: &mut Ui, label: &str, value: &mut Vec2) {
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut value.x).speed(0.5).prefix("x "));
        ui.add(egui::DragValue::new(&mut value.y).speed(0.5).prefix("y "));
        ui.label(label);
    });
}
