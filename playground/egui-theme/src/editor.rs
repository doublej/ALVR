//! Left panel: live controls for the theme + type scale.

use crate::params::ThemeParams;
use crate::typography::{TypeScale, ts_display};
use egui::{Color32, RichText, Ui, Vec2};
use std::ops::RangeInclusive;

pub fn show(ui: &mut Ui, p: &mut ThemeParams, scale: &mut TypeScale) {
    // The editor is a tool, not the product: keep its own controls compact and
    // readable regardless of how chunky the product theme is being tuned.
    let s = ui.spacing_mut();
    s.slider_width = 148.0;
    s.interact_size = Vec2::splat(20.0);
    s.item_spacing = Vec2::new(8.0, 6.0);
    s.button_padding = Vec2::new(8.0, 3.0);

    ui.label(RichText::new("Theme studio").text_style(ts_display()).size(18.0));
    ui.add_space(6.0);
    if ui.button("⟲  Reset to defaults").clicked() {
        *p = ThemeParams::default();
        *scale = TypeScale::default();
    }
    ui.add_space(8.0);

    egui::CollapsingHeader::new("Typography").default_open(true).show(ui, |ui| {
        slider(ui, "Display", &mut scale.display, 16.0..=40.0);
        slider(ui, "Heading", &mut scale.heading, 12.0..=24.0);
        slider(ui, "Body", &mut scale.body, 11.0..=18.0);
        slider(ui, "Small", &mut scale.small, 9.0..=15.0);
        slider(ui, "Mono", &mut scale.mono, 10.0..=18.0);
        slider(ui, "Button", &mut scale.button, 11.0..=18.0);
        slider(ui, "Eyebrow", &mut scale.eyebrow, 8.0..=14.0);
    });

    egui::CollapsingHeader::new("Accent & status").default_open(true).show(ui, |ui| {
        color(ui, "Accent", &mut p.accent);
        color(ui, "Accent hover", &mut p.accent_hover);
        color(ui, "On accent", &mut p.on_accent);
        color(ui, "OK green", &mut p.ok_green);
        color(ui, "KO red", &mut p.ko_red);
        color(ui, "Warning", &mut p.warning);
    });

    egui::CollapsingHeader::new("Surfaces").default_open(true).show(ui, |ui| {
        color(ui, "App background", &mut p.bg);
        color(ui, "Elevated", &mut p.elevated_bg);
        color(ui, "Section / card", &mut p.section_bg);
        color(ui, "Well / input", &mut p.darker_bg);
        color(ui, "Hover", &mut p.hover_bg);
    });

    egui::CollapsingHeader::new("Text & lines").default_open(false).show(ui, |ui| {
        color(ui, "Text primary", &mut p.fg);
        color(ui, "Text muted", &mut p.text_muted);
        color(ui, "Text faint", &mut p.text_faint);
        color(ui, "Separator", &mut p.separator);
        color(ui, "Border", &mut p.border);
    });

    egui::CollapsingHeader::new("Shape & spacing").default_open(false).show(ui, |ui| {
        u8_slider(ui, "Widget radius", &mut p.widget_radius, 0..=20);
        u8_slider(ui, "Card radius", &mut p.card_radius, 0..=24);
        slider(ui, "Card padding", &mut p.frame_padding, 4.0..=28.0);
        slider(ui, "Text spacing", &mut p.frame_text_spacing, 0.0..=20.0);
        slider(ui, "Interact size", &mut p.interact_size, 18.0..=44.0);
        slider(ui, "Slider width", &mut p.slider_width, 80.0..=320.0);
        vec2(ui, "Item spacing", &mut p.item_spacing);
        vec2(ui, "Button padding", &mut p.button_padding);
    });

    egui::CollapsingHeader::new("Log colors").default_open(false).show(ui, |ui| {
        color(ui, "Error", &mut p.log_error);
        color(ui, "Warning", &mut p.log_warning);
        color(ui, "Info", &mut p.log_info);
        color(ui, "Debug", &mut p.log_debug);
        color(ui, "Event", &mut p.log_event);
    });
}

fn color(ui: &mut Ui, label: &str, value: &mut Color32) {
    ui.horizontal(|ui| {
        ui.color_edit_button_srgba(value);
        ui.label(label);
    });
}

fn slider(ui: &mut Ui, label: &str, value: &mut f32, range: RangeInclusive<f32>) {
    ui.add(egui::Slider::new(value, range).max_decimals(1).text(label));
}

fn u8_slider(ui: &mut Ui, label: &str, value: &mut u8, range: RangeInclusive<u8>) {
    ui.add(egui::Slider::new(value, range).text(label));
}

fn vec2(ui: &mut Ui, label: &str, value: &mut Vec2) {
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut value.x).speed(0.5).prefix("x "));
        ui.add(egui::DragValue::new(&mut value.y).speed(0.5).prefix("y "));
        ui.label(label);
    });
}
