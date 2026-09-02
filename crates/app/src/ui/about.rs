//! About tab (Stage 5 T5.1)
//!
//! - Application version + short description
//! - Ko-fi supporter link (opt-in monetary support, no license gating)
//! - Credits / license note

use egui::Ui;

/// Ko-fi supporter link — points at the public Ko-fi account under
/// Moroya Sakamoto ` ko-fi.com/sakamoro `
///
/// **Do NOT** use `ko-fi.com/secrettreasurechest`; that is the STC
/// anonymous side account (adult-only). See personal memory
/// `reference_ko-fi_accounts.md` for the 2-account split rationale
pub const KOFI_URL: &str = "https://ko-fi.com/sakamoro";

pub fn show(ui: &mut Ui) {
    ui.heading("About text-to-print");
    ui.separator();

    // Content 全体を ScrollArea で wrap (小 viewport でも下部到達可能、2026-09-02 fix)
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.label(format!("Version: v{} BETA", env!("CARGO_PKG_VERSION")));
            ui.colored_label(
                ui.style().visuals.warn_fg_color,
                "BETA バージョンのため、text-to-print のリポジトリは Private となっています",
            );
            ui.add_space(8.0);

            ui.label(
                "text-to-print は自然言語プロンプトから LOL DSL を生成し、Bambu Lab 3MF を作成する \
                 スタンドアローン desktop アプリケーションです",
            );
            ui.add_space(12.0);

            ui.collapsing("Support the project", |ui| {
                ui.label(
                    "text-to-print は OSS です Free tier で恒久的に利用できます 開発を応援したい \
                     場合は Ko-fi で少額サポートを受け付けています (任意)",
                );
                ui.add_space(4.0);
                ui.hyperlink_to("☕ Support on Ko-fi", KOFI_URL);
            });

            ui.add_space(8.0);

            ui.collapsing("Credits", |ui| {
                ui.label("Author: Moroya Sakamoto <sakamoro@alicelaw.net>");
                ui.label("License: MIT");
                ui.label("Repository: https://github.com/ext-sakamoro/text-to-print (Private during BETA)");
                ui.add_space(4.0);
                // 3rd party OSS deps のみ列挙 (ALICE-* internal deps は同 author 内で
                // 「Depends on」の意味付けが薄いので削除)
                ui.label("Third-party OSS:");
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
        // Guard against accidental swap to the STC 18+ side account
        // (`ko-fi.com/secrettreasurechest`) — see personal memory
        // `reference_ko-fi_accounts.md`
        assert!(KOFI_URL.starts_with("https://"));
        assert_eq!(KOFI_URL, "https://ko-fi.com/sakamoro");
        assert!(!KOFI_URL.contains("secrettreasurechest"));
    }
}
