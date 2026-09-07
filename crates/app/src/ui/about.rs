//! About tab (Stage 5 T5.1)
//!
//! - Application version + short description
//! - Ko-fi supporter link (opt-in monetary support, no license gating)
//! - Credits / license note

use egui::Ui;

use crate::i18n::{Lang, T};

/// Ko-fi supporter link — points at the public Ko-fi account under
/// Moroya Sakamoto ` ko-fi.com/sakamoro `
///
/// **Do NOT** use `ko-fi.com/secrettreasurechest`; that is a separate
/// side account
pub const KOFI_URL: &str = "https://ko-fi.com/sakamoro";

pub fn show(ui: &mut Ui, lang: Lang) {
    ui.heading(T::about_heading(lang));
    ui.separator();

    // Content 全体を ScrollArea で wrap (小 viewport でも下部到達可能、2026-09-02 fix)
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.label(format!(
                "{}: v{} BETA",
                T::about_version(lang),
                env!("CARGO_PKG_VERSION")
            ));
            ui.colored_label(
                ui.style().visuals.warn_fg_color,
                T::about_beta_warning(lang),
            );
            ui.add_space(8.0);

            ui.label(T::about_description(lang));
            ui.add_space(12.0);

            ui.collapsing(T::about_support_header(lang), |ui| {
                ui.label(T::about_support_body(lang));
                ui.add_space(4.0);
                ui.hyperlink_to(T::about_support_kofi(lang), KOFI_URL);
            });

            ui.add_space(8.0);

            ui.collapsing(T::about_credits(lang), |ui| {
                ui.label(T::about_credits_author(lang));
                ui.label(T::about_credits_license(lang));
                ui.label(T::about_credits_repo(lang));
                ui.add_space(4.0);
                ui.label(T::about_credits_deps(lang));
                ui.label("  · egui / eframe / wgpu (GUI)");
                ui.label("  · libp2p (P2P share)");
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kofi_url_points_to_public_sakamoro_account() {
        // Guard against accidental swap to alternate side accounts
        assert!(KOFI_URL.starts_with("https://"));
        assert_eq!(KOFI_URL, "https://ko-fi.com/sakamoro");
        assert!(!KOFI_URL.contains("secrettreasurechest"));
    }
}
