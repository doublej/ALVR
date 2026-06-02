//! ALVR egui theme playground.
//!
//! Standalone eframe app for experimenting with the dashboard's egui style.
//! Edit colors / spacing on the left, see the live preview in the middle, and
//! copy paste-ready `theme.rs` code from the right. See README.md.

mod editor;
mod gallery;
mod params;

use eframe::egui;
use gallery::DemoState;
use params::ThemeParams;

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1180.0, 780.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ALVR egui Theme Playground",
        options,
        Box::new(|_cc| Ok(Box::<App>::default())),
    )
}

#[derive(Default)]
struct App {
    params: ThemeParams,
    demo: DemoState,
}

impl eframe::App for App {
    // egui 0.34: the per-frame method is `ui` (receives a `&mut Ui` directly).
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Re-apply every frame so edits are immediately visible.
        self.params.apply(ui.ctx());

        egui::Panel::left("editor")
            .resizable(true)
            .default_size(340.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| editor::show(ui, &mut self.params));
            });

        egui::Panel::right("export")
            .resizable(true)
            .default_size(380.0)
            .show_inside(ui, |ui| {
                ui.heading("Export → theme.rs");
                if ui.button("📋 Copy code").clicked() {
                    ui.ctx().copy_text(self.params.export_rust());
                }
                ui.separator();
                let mut code = self.params.export_rust();
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut code)
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical()
                .show(ui, |ui| gallery::show(ui, &mut self.demo, &self.params));
        });
    }
}
