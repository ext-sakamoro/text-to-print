//! Gallery tab (Phase 3、2026-08-26)
//!
//! Reads [`AppState::gallery`] populated by
//! [`crate::state::spawn_gallery_fetch`] (Cloudflare Worker
//! `GET /api/gallery/list`) The old P2P `AliceNode::list_cached_sdfs`
//! path is kept for reference only — the Cloudflare relay is the
//! canonical source for β
//!
//! ## UI shape
//!
//! - Left: list of Gallery items (nickname or DID short-form label +
//!   date, click to select)
//! - Right: detail panel with the LOL source, a preview button, a fork
//!   input, and (own posts only) a delete button
//! - Top bar: refresh button + count + fetch status

use egui::Ui;

use crate::i18n::{Lang, T};
use crate::state::{AppState, GalleryLoadState};
use crate::ui::viewer::SdfViewer;
use text_to_print_core::pipeline::{self, Quality};
use text_to_print_network::gallery_client::{GalleryClient, GalleryItem};
use text_to_print_network::node::AliceNode;

/// Local UI state for the Gallery tab (selection, fork buffer, tab flip)
#[derive(Default)]
pub struct GalleryState {
    pub selected_id: Option<String>,
    pub fork_input: String,
    pub switch_to_viewer: bool,
    /// Set to true after a delete succeeds so the Gallery tab triggers a
    /// refresh next frame (avoids re-render of the just-deleted row)
    pub refresh_after_delete: bool,
    /// 2026-09-04 案 A: 「編集して再生成」button click 時にセットする LOL
    /// main.rs が Generate tab に切替 + prompt_input に流し込み + 実験機能
    /// section を expand して user がすぐ edit 可能な状態にする
    pub edit_lol_pending: Option<String>,
}

pub fn show(
    ui: &mut Ui,
    state: &mut AppState,
    node: &AliceNode,
    viewer: &mut SdfViewer,
    gallery: &mut GalleryState,
    lang: Lang,
) {
    ui.heading(T::gallery_heading(lang));
    ui.separator();

    // Kick off a refresh if a delete just succeeded (clears the deleted
    // row without waiting for the user to click refresh manually)
    if gallery.refresh_after_delete {
        gallery.refresh_after_delete = false;
        gallery.selected_id = None;
        spawn_gallery_refresh(state);
    }

    // Top bar: fetch status + refresh button
    ui.horizontal(|ui| {
        match &state.gallery {
            Some(GalleryLoadState::Loaded(items)) => {
                ui.label(format!(
                    "{}: {} {}",
                    T::gallery_public_count(lang),
                    items.len(),
                    T::gallery_items_suffix(lang)
                ));
            }
            Some(GalleryLoadState::Error(msg)) => {
                ui.colored_label(egui::Color32::LIGHT_RED, T::gallery_fetch_failed(lang));
                ui.label(egui::RichText::new(msg).small().weak());
            }
            None => {
                ui.label(T::gallery_loading(lang));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button(T::gallery_refresh_button(lang)).clicked() {
                spawn_gallery_refresh(state);
            }
        });
    });
    ui.add_space(4.0);

    let items = match &state.gallery {
        Some(GalleryLoadState::Loaded(v)) if !v.is_empty() => v.clone(),
        Some(GalleryLoadState::Loaded(_)) => {
            ui.add_space(16.0);
            ui.label(T::gallery_empty(lang));
            ui.label(T::gallery_empty_hint(lang));
            return;
        }
        Some(GalleryLoadState::Error(_)) => {
            ui.add_space(16.0);
            ui.label(T::gallery_network_error(lang));
            ui.label(T::gallery_network_hint(lang));
            return;
        }
        None => {
            ui.add_space(16.0);
            ui.spinner();
            return;
        }
    };

    let own_did = node.identity.did.id.clone();
    let available = ui.available_size();
    let list_width = (available.x * 0.4).min(400.0);

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(list_width);
            egui::ScrollArea::vertical()
                .id_salt("gallery_list")
                .show(ui, |ui| {
                    for sdf in &items {
                        show_list_row(ui, sdf, gallery);
                    }
                });
        });

        ui.separator();

        ui.vertical(|ui| {
            if let Some(selected_id) = gallery.selected_id.clone() {
                if let Some(sdf) = items.iter().find(|s| s.id == selected_id) {
                    show_detail(ui, sdf, &own_did, state, node, viewer, gallery, lang);
                } else {
                    gallery.selected_id = None;
                }
            } else {
                ui.label(T::gallery_select_prompt(lang));
            }
        });
    });
}

fn show_list_row(ui: &mut Ui, sdf: &GalleryItem, gallery: &mut GalleryState) {
    let is_selected = gallery.selected_id.as_deref() == Some(&sdf.id);
    let author = author_label(sdf);
    let created = &sdf.created_at[..10.min(sdf.created_at.len())];
    let label = format!("{author} — {created}");
    if ui.selectable_label(is_selected, label).clicked() {
        gallery.selected_id = Some(sdf.id.clone());
    }
}

fn show_detail(
    ui: &mut Ui,
    sdf: &GalleryItem,
    own_did: &str,
    state: &mut AppState,
    node: &AliceNode,
    viewer: &mut SdfViewer,
    gallery: &mut GalleryState,
    lang: Lang,
) {
    ui.heading(T::gallery_detail_heading(lang));
    ui.label(format!(
        "{}: {}",
        T::gallery_id_label(lang),
        &sdf.id[..16.min(sdf.id.len())]
    ));
    ui.label(format!(
        "{}: {}",
        T::gallery_author_label(lang),
        author_label(sdf)
    ));
    ui.label(format!(
        "{}: {}",
        T::gallery_did_label(lang),
        short_did(&sdf.author_did)
    ));
    ui.label(format!(
        "{}: {}",
        T::gallery_created_label(lang),
        sdf.created_at
    ));

    ui.add_space(8.0);
    ui.label(T::gallery_source_label(lang));
    let mut lol_display = sdf.lol_source.clone();
    ui.add(
        egui::TextEdit::multiline(&mut lol_display)
            .code_editor()
            .desired_rows(8),
    );

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button(T::gallery_show_preview(lang)).clicked() {
            spawn_preview_generation(state, sdf.lol_source.clone());
            let _ = viewer; // preview is now delivered via state.viewer_mesh
            gallery.switch_to_viewer = true;
        }
        if ui
            .button(T::gallery_edit_and_regen(lang))
            .on_hover_text(T::gallery_edit_and_regen_hover(lang))
            .clicked()
        {
            gallery.edit_lol_pending = Some(sdf.lol_source.clone());
            gallery.switch_to_viewer = true; // reuse tab flip flag
        }
        // Own-post delete Available only when the row's author_did
        // matches the local Identity — the Worker double-checks on the
        // server side (sig verify + author_did match)
        let is_own = sdf.author_did == own_did;
        if is_own {
            let btn = ui.button(T::gallery_delete_own(lang));
            if btn.clicked() {
                spawn_gallery_delete(state, node, sdf.id.clone(), sdf.author_did.clone());
                gallery.refresh_after_delete = true;
            }
        }
    });

    ui.add_space(8.0);
    ui.label(T::gallery_fork_prompt(lang));
    ui.text_edit_multiline(&mut gallery.fork_input);
    if ui.button(T::gallery_fork_publish(lang)).clicked() && !gallery.fork_input.trim().is_empty() {
        // Validate LOL locally before publish so the Worker doesn't
        // spend a rate-limit slot on obvious junk (parse error surfaces
        // in the log; retry after fix)
        let new_lol = gallery.fork_input.trim().to_string();
        match alice_bamboo::parse_lol(&new_lol) {
            Ok(_) => {
                spawn_gallery_publish(state, node, new_lol, state.nickname.clone());
                gallery.fork_input.clear();
                gallery.refresh_after_delete = true; // repurpose flag to trigger refresh
            }
            Err(e) => {
                tracing::warn!(error = %e, "invalid LOL for fork, publish aborted");
            }
        }
    }
}

/// Compose the display label for a Gallery author: nickname when present
/// (trim + non-empty), otherwise a short DID form so the row is still
/// visually distinguishable
fn author_label(sdf: &GalleryItem) -> String {
    sdf.author_nickname
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map_or_else(
            || short_did(&sdf.author_did),
            std::string::ToString::to_string,
        )
}

fn short_did(did: &str) -> String {
    let head = &did[..16.min(did.len())];
    let tail = &did[did.len().saturating_sub(6)..];
    format!("{head}...{tail}")
}

fn spawn_gallery_refresh(state: &AppState) {
    let endpoint = GalleryClient::resolve_endpoint();
    crate::state::spawn_gallery_fetch(&state.runtime, endpoint, state.result_tx.clone());
}

/// Generate a mesh from the LOL DSL and stage it in `AppState::viewer_mesh`
///
/// Runs inline on the UI thread — matches how the Generate tab
/// currently handles preview mesh handoff Mesh gen for a typical
/// Preview-quality item is ~1-5s on M-series (same order as generate
/// tab) A follow-up refactor to spawn on the runtime would need a new
/// `GenerationMessage` variant to hand the `Arc<Mesh>` back safely
/// without racing `App::update` reads β scope keeps it inline
fn spawn_preview_generation(state: &mut AppState, lol_source: String) {
    match pipeline::preview_lol_to_mesh(&lol_source, Quality::Preview) {
        Ok(mesh) => {
            state.viewer_mesh = Some(mesh);
            state.mesh_version = state.mesh_version.wrapping_add(1);
            tracing::info!("gallery preview mesh generated and staged for viewer");
        }
        Err(e) => {
            tracing::warn!(error = %e, "gallery preview mesh generation failed");
        }
    }
}

fn spawn_gallery_delete(state: &AppState, node: &AliceNode, id: String, author_did: String) {
    let endpoint = GalleryClient::resolve_endpoint();
    let signing_key = node.identity.signing_key().clone();
    let tx = state.result_tx.clone();
    state.runtime.spawn(async move {
        let client = GalleryClient::new(endpoint);
        match client.delete(&signing_key, &id, &author_did).await {
            Ok(()) => {
                tracing::info!(id, "gallery post deleted");
                // Trigger a fresh list fetch so the removed row disappears
                let refresh = GalleryClient::new(GalleryClient::resolve_endpoint());
                if let Ok(resp) = refresh.list(100, 0).await {
                    let _ = tx.send(crate::state::GenerationMessage::GalleryLoaded(
                        GalleryLoadState::Loaded(resp.items),
                    ));
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, id, "gallery delete failed");
            }
        }
    });
}

pub(crate) fn spawn_gallery_publish(
    state: &AppState,
    node: &AliceNode,
    lol_source: String,
    nickname: String,
) {
    let endpoint = GalleryClient::resolve_endpoint();
    let signing_key = node.identity.signing_key().clone();
    let author_did = node.identity.did.id.clone();
    let tx = state.result_tx.clone();
    state.runtime.spawn(async move {
        let client = GalleryClient::new(endpoint);
        let id = uuid::Uuid::now_v7().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        let nick = if nickname.trim().is_empty() {
            None
        } else {
            Some(nickname.as_str())
        };
        match client
            .publish(
                &signing_key,
                &id,
                &author_did,
                nick,
                &lol_source,
                &created_at,
            )
            .await
        {
            Ok(()) => {
                tracing::info!(id, "gallery post published (fork)");
                let refresh = GalleryClient::new(GalleryClient::resolve_endpoint());
                if let Ok(resp) = refresh.list(100, 0).await {
                    let _ = tx.send(crate::state::GenerationMessage::GalleryLoaded(
                        GalleryLoadState::Loaded(resp.items),
                    ));
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, id, "gallery publish failed");
            }
        }
    });
}
