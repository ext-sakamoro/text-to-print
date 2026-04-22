use egui::Ui;
use tdvbgaran_network::node::AliceNode;

use crate::ui::viewer::SdfViewer;

/// Gallery で選択された SDF の情報
#[derive(Default)]
pub struct GalleryState {
    pub selected_id: Option<String>,
    pub fork_input: String,
    pub switch_to_viewer: bool,
}

pub fn show(ui: &mut Ui, node: &mut AliceNode, viewer: &mut SdfViewer, gallery: &mut GalleryState) {
    ui.heading("Gallery");
    ui.separator();

    let sdfs = node.list_cached_sdfs();

    if sdfs.is_empty() {
        ui.add_space(16.0);
        ui.label("まだ公開 SDF がありません");
        ui.label("General tier ユーザーの生成データがここに表示されます");

        ui.add_space(8.0);
        ui.label(format!("DAG ノード数: {}", node.dag_node_count()));
        return;
    }

    ui.horizontal(|ui| {
        ui.label(format!("{} 件の公開 SDF", sdfs.len()));
        ui.label("|");
        ui.label(format!("DAG: {} ノード", node.dag_node_count()));
    });

    ui.add_space(4.0);

    // 左: SDF 一覧、右: 選択された SDF の詳細
    let available = ui.available_size();
    let list_width = (available.x * 0.4).min(400.0);

    ui.horizontal(|ui| {
        // 左パネル: SDF 一覧
        ui.vertical(|ui| {
            ui.set_width(list_width);
            egui::ScrollArea::vertical()
                .id_salt("gallery_list")
                .show(ui, |ui| {
                    for sdf in &sdfs {
                        let is_selected = gallery.selected_id.as_deref() == Some(&sdf.id);
                        let label = format!(
                            "{} | {}",
                            &sdf.id[..8.min(sdf.id.len())],
                            &sdf.created_at[..10.min(sdf.created_at.len())]
                        );

                        if ui.selectable_label(is_selected, &label).clicked() {
                            gallery.selected_id = Some(sdf.id.clone());
                            viewer.set_lol(&sdf.lol_source);
                        }
                    }
                });
        });

        ui.separator();

        // 右パネル: 選択 SDF の詳細
        ui.vertical(|ui| {
            if let Some(selected_id) = &gallery.selected_id {
                if let Some(sdf) = sdfs.iter().find(|s| &s.id == selected_id) {
                    ui.heading("詳細");

                    ui.label(format!("ID: {}", &sdf.id[..16.min(sdf.id.len())]));
                    ui.label(format!(
                        "Author: {}...{}",
                        &sdf.author_did[..12.min(sdf.author_did.len())],
                        &sdf.author_did[sdf.author_did.len().saturating_sub(6)..]
                    ));
                    ui.label(format!("Created: {}", &sdf.created_at));

                    ui.add_space(8.0);
                    ui.label("LOL ソース:");
                    ui.add(
                        egui::TextEdit::multiline(&mut sdf.lol_source.clone())
                            .code_editor()
                            .desired_rows(8),
                    );

                    ui.add_space(4.0);
                    if ui.button("3D プレビューで表示").clicked() {
                        viewer.set_lol(&sdf.lol_source);
                        gallery.switch_to_viewer = true;
                    }

                    ui.add_space(8.0);

                    // フォークボタン
                    ui.label("フォーク (リミックス):");
                    ui.text_edit_multiline(&mut gallery.fork_input);

                    if ui.button("フォークして公開").clicked()
                        && !gallery.fork_input.trim().is_empty()
                    {
                        // alice_lol で検証
                        let new_lol = gallery.fork_input.trim();
                        if alice_lol::runtime_parser::parse_lol(new_lol).is_ok() {
                            node.fork_sdf(&sdf.id, new_lol);
                            viewer.set_lol(new_lol);
                            gallery.fork_input.clear();
                            tracing::info!(
                                original = %sdf.id[..8.min(sdf.id.len())],
                                "SDF forked"
                            );
                        } else {
                            tracing::warn!("invalid LOL for fork");
                        }
                    }
                } else {
                    gallery.selected_id = None;
                }
            } else {
                ui.label("SDF を選択してください");
            }
        });
    });
}
