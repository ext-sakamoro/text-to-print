//! About tab (Stage 5 T5.1)
//!
//! - Application version + short description
//! - Ko-fi supporter link (opt-in monetary support, no license gating)
//! - Credits / license note

use egui::Ui;

/// Ko-fi supporter link Points at the shared Secret-Treasure-Chest Ko-fi
/// (see `~/CLAUDE.md` §Secret-Treasure-Chest for provenance)
pub const KOFI_URL: &str = "https://ko-fi.com/secrettreasurechest";

pub fn show(ui: &mut Ui) {
    ui.heading("About text-to-print");
    ui.separator();

    ui.label(format!("Version: v{}", env!("CARGO_PKG_VERSION")));
    ui.add_space(8.0);

    ui.label(
        "text-to-print は自然言語プロンプトから LOL DSL を生成し、Bambu Lab 3MF を作成する \
         スタンドアローン desktop アプリケーションです ALICE-Eco-System (ALICE-SDF / \
         ALICE-LOL / ALICE-Bamboo / ALICE-Physics / ALICE-LLM) を統合しています",
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
        ui.label("Repository: https://github.com/ext-sakamoro/text-to-print");
        ui.add_space(4.0);
        ui.label("Depends on:");
        ui.label("  · alice-sdf / alice-lol / alice-view (ALICE-Eco-System)");
        ui.label("  · alice-bamboo / alice-physics (Stage 4)");
        ui.label("  · alice-llm (embedded inference)");
        ui.label("  · egui / eframe / wgpu (GUI)");
        ui.label("  · libp2p (P2P share)");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kofi_url_is_https() {
        assert!(KOFI_URL.starts_with("https://"));
        assert!(KOFI_URL.contains("ko-fi.com"));
    }
}
