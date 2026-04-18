use egui::Ui;

use crate::state::AppState;

pub fn show(ui: &mut Ui, state: &mut AppState) {
    ui.heading("生成履歴");
    ui.separator();

    if ui.button("更新").clicked() {
        state.refresh_history();
    }

    ui.add_space(8.0);

    if state.history.is_empty() {
        ui.label("まだ生成履歴がありません");
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
                        ui.label("(公開)");
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
