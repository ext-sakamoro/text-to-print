use egui::Ui;
use tdvbgaran_network::node::AliceNode;

pub fn show(ui: &mut Ui, node: &AliceNode) {
    ui.heading("Gallery");
    ui.separator();
    ui.label("P2P ネットワーク上の公開 SDF");

    let sdfs = node.list_cached_sdfs();

    if sdfs.is_empty() {
        ui.add_space(16.0);
        ui.label("まだ公開 SDF がありません");
        ui.label("General tier ユーザーの生成データがここに表示されます");
        return;
    }

    ui.add_space(8.0);
    ui.label(format!("{} 件の公開 SDF", sdfs.len()));
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for sdf in &sdfs {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(&sdf.id[..8.min(sdf.id.len())]);
                    ui.label("|");
                    ui.label(&sdf.created_at[..10.min(sdf.created_at.len())]);
                });

                ui.label(format!("Author: {}...{}", &sdf.author_did[..12], &sdf.author_did[sdf.author_did.len().saturating_sub(6)..]));

                ui.collapsing("LOL ソース", |ui| {
                    ui.monospace(&sdf.lol_source);
                });
            });
            ui.add_space(4.0);
        }
    });
}
