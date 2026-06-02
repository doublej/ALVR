//! ALVR egui theme playground.
//!
//! Standalone eframe app for designing the dashboard's egui look — typography,
//! color and elements — then exporting paste-ready `theme.rs`. See README.md.

mod editor;
mod gallery;
mod params;
mod typography;

use eframe::egui;
use egui::{Frame, Margin, Mesh, Shape};
use gallery::DemoState;
use params::ThemeParams;
use typography::TypeScale;

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 820.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ALVR Theme Studio",
        options,
        Box::new(|cc| {
            typography::install_fonts(&cc.egui_ctx);
            Ok(Box::<App>::default())
        }),
    )
}

#[derive(Default)]
struct App {
    theme: ThemeParams,
    scale: TypeScale,
    demo: DemoState,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.theme.apply(ui.ctx(), &self.scale);
        // `ctx.set_style` only affects newly-created Uis; the Ui eframe handed us
        // (and its panel children) still carries the frame-start style. Pull the
        // freshly-applied style onto it so custom text styles/visuals are live now.
        let style = ui.ctx().style();
        ui.set_style(style);

        let panel_frame = Frame::new()
            .fill(self.theme.elevated_bg)
            .inner_margin(Margin::same(16));

        egui::Panel::left("editor")
            .resizable(true)
            .default_size(364.0)
            .frame(panel_frame.clone())
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .show(ui, |ui| editor::show(ui, &mut self.theme, &mut self.scale));
            });

        egui::Panel::right("export")
            .resizable(true)
            .default_size(400.0)
            .frame(panel_frame)
            .show_inside(ui, |ui| export_panel(ui, &self.theme));

        egui::CentralPanel::default().show_inside(ui, |ui| {
            backdrop(ui, &self.theme);
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(6.0);
                // Fixed, centered measure — controls read as an instrument panel,
                // not a form rubber-banded to the window width.
                let avail = ui.available_width();
                let measure = avail.min(860.0);
                let pad = ((avail - measure) / 2.0).max(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(pad);
                    ui.vertical(|ui| {
                        ui.set_max_width(measure);
                        ui.set_width(measure);
                        gallery::show(ui, &mut self.demo, &self.theme);
                    });
                });
                ui.add_space(8.0);
            });
        });
    }
}

fn export_panel(ui: &mut egui::Ui, theme: &ThemeParams) {
    ui.label(egui::RichText::new("Export").text_style(typography::ts_display()).size(18.0));
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Paste into alvr/gui_common/src/theme.rs").color(theme.text_muted).size(12.0));
    ui.add_space(6.0);
    if ui.button("📋  Copy theme.rs").clicked() {
        ui.ctx().copy_text(theme.export_rust());
    }
    ui.add_space(8.0);
    let mut code = theme.export_rust();
    egui::ScrollArea::both().show(ui, |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut code)
                .code_editor()
                .desired_width(f32::INFINITY),
        );
    });
}

/// Subtle top-down gradient so the central area has depth, not a flat fill.
fn backdrop(ui: &mut egui::Ui, theme: &ThemeParams) {
    let rect = ui.max_rect();
    let top = theme.bg.lerp_to_gamma(theme.elevated_bg, 0.7);
    let bottom = theme.bg;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
}
