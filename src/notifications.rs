use eframe::egui;
use eframe::egui::ViewportBuilder;
use eframe::egui::ViewportClass;
use eframe::egui::ViewportId;
use std::time::Instant;

use crate::Unfocol;
use crate::message::Severity;

impl Unfocol<fn() -> Instant> {
    /// Shows a viewport listing all pending [`Message`](crate::Message)s, if
    /// there are any, and clears them once the viewport is dismissed
    /// (Escape or close).
    pub fn display_notifications(&mut self, ctx: &egui::Context) {
        if !self.messages.is_empty() {
            let viewport_id = ViewportId::from_hash_of("notifications");
            let builder = ViewportBuilder::default()
                .with_app_id("se.johnkinell.Unfocol.Notifications")
                .with_title("Unfocol notifications")
                .with_active(true)
                .with_decorations(true)
                .with_close_button(true)
                .with_inner_size([480.0, 200.0])
                .with_min_inner_size([50.0, 50.0]);
            let _class = ViewportClass::Immediate;

            ctx.show_viewport_immediate(viewport_id, builder, |ui, _class| {
                if let Some(cmd) = egui::ViewportCommand::center_on_screen(ctx) {
                    ctx.send_viewport_cmd(cmd);
                }
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            for message in &self.messages {
                                ui.horizontal(|ui| {
                                    match &message.severity {
                                        Severity::Error => ui.label("Error"),
                                        Severity::Warning => ui.label("Warning"),
                                        Severity::Info => ui.label("Info"),
                                    };
                                    ui.add(
                                        egui::Label::new(&message.message)
                                            .wrap_mode(egui::TextWrapMode::Wrap)
                                    );
                                });
                                ui.separator();
                            }
                        });
                    let should_close = ctx.input(|i| {
                        i.key_pressed(egui::Key::Escape) || i.viewport().close_requested()
                    });
                    if should_close {
                        self.messages.clear();
                    }
                });
            });
        }
    }
}
