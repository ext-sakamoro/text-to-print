//! Gallery Phase 2 (2026-08-26) share-confirm modal
//!
//! Renders when [`AppState::pending_share_confirm`] is populated The
//! async generation task dumps the dry-run payload for local audit but
//! defers the real `enqueue` until this dialog resolves it
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
use text_to_print_network::identity::Identity;

use crate::i18n::{Lang, T};
use crate::state::AppState;

/// Render the modal if a payload is awaiting confirmation No-op
/// otherwise Called once per frame from `App::update`
pub fn show(ctx: &Context, state: &mut AppState, identity: &Identity, lang: Lang) {
    let Some(path) = state.pending_share_confirm.clone() else {
        return;
    };

    let mut resolution: Option<Resolution> = None;

    egui::Window::new(T::share_confirm_title(lang))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.label(T::share_confirm_body(lang));
            ui.add_space(4.0);
            ui.label(T::share_confirm_visibility(lang));
            ui.label(T::share_confirm_audit(lang));
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui.button(T::share_confirm_publish_once(lang)).clicked() {
                    resolution = Some(Resolution::PublishOnce);
                }
                if ui.button(T::share_confirm_skip(lang)).clicked() {
                    resolution = Some(Resolution::Skip);
                }
            });
            ui.add_space(4.0);
            if ui
                .button(T::share_confirm_always(lang))
                .on_hover_text(T::share_confirm_always_hover(lang))
                .clicked()
            {
                resolution = Some(Resolution::AlwaysPublish);
            }
        });

    if let Some(res) = resolution {
        apply(state, identity, &path, res);
    }
}

#[derive(Debug, Clone, Copy)]
enum Resolution {
    PublishOnce,
    Skip,
    AlwaysPublish,
}

fn apply(state: &mut AppState, identity: &Identity, dry_run_path: &PathBuf, res: Resolution) {
    match res {
        Resolution::Skip => {
            state.pending_share_confirm = None;
        }
        Resolution::PublishOnce => {
            enqueue_from_dry_run(state, identity, dry_run_path);
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
            enqueue_from_dry_run(state, identity, dry_run_path);
            state.pending_share_confirm = None;
        }
    }
}

fn enqueue_from_dry_run(state: &AppState, identity: &Identity, dry_run_path: &PathBuf) {
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
    if let Err(e) = enqueue(&payload, &queue_dir, ShareConsent::pressed_publish()) {
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
        identity,
        payload.lol_source.clone(),
        state.nickname.clone(),
    );
}

/// The user's agreement to send one payload [`enqueue`] takes one, so
/// nothing reaches the upload queue unless the user agreed
///
/// There are exactly two ways to agree:
/// - the user pressed publish in this dialog for this payload
///   (`pressed_publish`, private to this module, so only the dialog's
///   publish path can make one)
/// - the user turned on auto-publish themselves, in Settings or with the
///   dialog's "always" button ([`ShareConsent::standing`]) It is off by
///   default and nothing else turns it on
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShareConsent {
    _private: (),
}

impl ShareConsent {
    /// The user pressed publish for this payload in this dialog
    const fn pressed_publish() -> Self {
        Self { _private: () }
    }

    /// The user's auto-publish setting `None` while it is off, so a
    /// generation can only be queued without a press when the user chose
    /// that
    #[must_use]
    pub(crate) fn standing(auto_publish_on: bool) -> Option<Self> {
        auto_publish_on.then_some(Self { _private: () })
    }
}

/// Persist a payload to the pending queue directory so a later
/// `retry_queued_uploads` sweep can attempt delivery Needs the user's
/// [`ShareConsent`]
///
/// # Errors
///
/// - IO error creating the directory or writing the file
/// - Serialization error from `serde_json`
pub(crate) fn enqueue(
    payload: &text_to_print_network::share::SharePayload,
    queue_dir: &std::path::Path,
    _consent: ShareConsent,
) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(queue_dir)?;
    let path = text_to_print_network::share::queue_file_path(payload, queue_dir);
    std::fs::write(&path, serde_json::to_string_pretty(payload)?)?;
    Ok(path)
}

#[cfg(test)]
mod consent_tests {
    use super::ShareConsent;

    /// 自動公開が off なら同意は作れない = 押していない共有は queue に入らない
    #[test]
    fn auto_publish_off_gives_no_consent() {
        assert_eq!(ShareConsent::standing(false), None);
        assert_eq!(
            ShareConsent::standing(true),
            Some(ShareConsent::pressed_publish())
        );
    }
}
