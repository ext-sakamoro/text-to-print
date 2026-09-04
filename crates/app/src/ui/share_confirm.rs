//! Gallery Phase 2 (2026-08-26) share-confirm modal
//!
//! Renders when [`AppState::pending_share_confirm`] is populated The
//! async generation task dumps the dry-run payload for local audit but
//! defers the real `share::enqueue()` until this dialog resolves it
//!
//! Three exits:
//! - **Publish once**: read the dry-run JSON, deserialise, enqueue for
//!   the Cloudflare Worker sweep, clear `pending_share_confirm`
//! - **Skip**: clear `pending_share_confirm` (dry-run stays on disk for
//!   local audit but nothing is queued for real upload)
//! - **Always publish**: flip `gallery_auto_share` on in DB + state,
//!   then take the publish-once path Future generations bypass this
//!   dialog until the user opts back out in Settings

use egui::Context;
use std::path::PathBuf;
use text_to_print_network::node::AliceNode;

use crate::state::AppState;

/// Render the modal if a payload is awaiting confirmation No-op
/// otherwise Called once per frame from `App::update`
pub fn show(ctx: &Context, state: &mut AppState, node: &AliceNode) {
    let Some(path) = state.pending_share_confirm.clone() else {
        return;
    };

    let mut resolution: Option<Resolution> = None;

    egui::Window::new("Gallery に公開しますか?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label("この生成物 (LOL DSL + prompt + 品質シグナル) を Gallery に公開しますか?");
            ui.add_space(4.0);
            ui.label("公開すると他の user から fork / 参考にされる可能性があります");
            ui.label("公開しなくても local audit ログには残ります (Settings から確認可)");
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui.button("今回だけ公開").clicked() {
                    resolution = Some(Resolution::PublishOnce);
                }
                if ui.button("公開しない").clicked() {
                    resolution = Some(Resolution::Skip);
                }
            });
            ui.add_space(4.0);
            if ui
                .button("毎回自動公開 (以降 dialog 出ない)")
                .on_hover_text(
                    "Settings > プロフィール からいつでも off に戻せます auto 中も Paid tier に変更すれば自動で upload 停止",
                )
                .clicked()
            {
                resolution = Some(Resolution::AlwaysPublish);
            }
        });

    if let Some(res) = resolution {
        apply(state, node, &path, res);
    }
}

#[derive(Debug, Clone, Copy)]
enum Resolution {
    PublishOnce,
    Skip,
    AlwaysPublish,
}

fn apply(state: &mut AppState, node: &AliceNode, dry_run_path: &PathBuf, res: Resolution) {
    match res {
        Resolution::Skip => {
            state.pending_share_confirm = None;
        }
        Resolution::PublishOnce => {
            enqueue_from_dry_run(state, node, dry_run_path);
            state.pending_share_confirm = None;
        }
        Resolution::AlwaysPublish => {
            // Persist the preference first so a crash between DB write
            // and the enqueue leaves the user with the setting they
            // clicked, not the pre-click state
            if let Err(e) = state.db.set_gallery_auto_share(&state.profile_id, true) {
                tracing::warn!(error = %e, "failed to persist gallery_auto_share=true");
            } else {
                state.gallery_auto_share = true;
            }
            enqueue_from_dry_run(state, node, dry_run_path);
            state.pending_share_confirm = None;
        }
    }
}

fn enqueue_from_dry_run(state: &AppState, node: &AliceNode, dry_run_path: &PathBuf) {
    let bytes = match std::fs::read(dry_run_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(path = %dry_run_path.display(), error = %e, "share dry-run read failed");
            return;
        }
    };
    let payload: text_to_print_network::share::SharePayload = match serde_json::from_slice(&bytes) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(path = %dry_run_path.display(), error = %e, "share dry-run parse failed");
            return;
        }
    };
    // 1. LoRA training queue (Phase 2 /api/share) — 従来経路
    let queue_dir = state.share_queue_dir();
    if let Err(e) = text_to_print_network::share::enqueue(&payload, &queue_dir) {
        tracing::warn!(error = %e, "share enqueue-on-confirm failed");
    } else {
        tracing::info!(uuid = %payload.uuid, "share enqueued via confirm dialog");
    }
    // 2. Gallery public share (Phase 3 /api/gallery/publish) — modal 経路の追加配線
    // 2026-09-04 fix: Phase 2 → Phase 3 統合時に modal から gallery publish
    // trigger 忘れの bug 修正 fork action 以外にも modal の「公開する」で
    // Gallery に反映される必要がある
    crate::ui::gallery::spawn_gallery_publish(
        state,
        node,
        payload.lol_source.clone(),
        state.nickname.clone(),
    );
}
