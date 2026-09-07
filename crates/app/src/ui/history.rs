use egui::Ui;

use crate::i18n::{Lang, T};
use crate::state::AppState;

pub fn show(ui: &mut Ui, state: &mut AppState, lang: Lang) {
    ui.heading(T::generation_history(lang));
    ui.separator();

    if ui.button(T::refresh(lang)).clicked() {
        state.refresh_history();
    }

    ui.add_space(8.0);

    if state.history.is_empty() {
        ui.label(T::no_history(lang));
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for row in &state.history {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    let status_color = match row.status.as_str() {
                        "complete" => egui::Color32::GREEN,
                        "error" => egui::Color32::RED,
                        _ => egui::Color32::YELLOW,
                    };
                    ui.colored_label(status_color, &row.status);
                    ui.label(&row.created_at);
                    if row.is_public {
                        ui.label(T::published_tag(lang));
                    }
                });

                ui.label(&row.prompt);

                if let Some(lol) = &row.lol_source {
                    ui.collapsing("LOL", |ui| {
                        ui.monospace(lol);
                    });
                }
            });
            ui.add_space(4.0);
        }
    });
}
