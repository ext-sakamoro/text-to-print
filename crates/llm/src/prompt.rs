/// LOL DSL 生成用のシステムプロンプト
pub const SYSTEM_PROMPT: &str = include_str!("system_prompt.md");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_is_not_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
    }

    #[test]
    fn system_prompt_contains_lol_keyword() {
        assert!(SYSTEM_PROMPT.contains("LOL"));
    }

    #[test]
    fn system_prompt_contains_primitives() {
        assert!(SYSTEM_PROMPT.contains("sphere"));
        assert!(SYSTEM_PROMPT.contains("box3d"));
        assert!(SYSTEM_PROMPT.contains("cylinder"));
    }

    #[test]
    fn system_prompt_contains_operations() {
        assert!(SYSTEM_PROMPT.contains("union"));
        assert!(SYSTEM_PROMPT.contains("subtract"));
        assert!(SYSTEM_PROMPT.contains("smooth_union"));
    }

    #[test]
    fn system_prompt_contains_print_constraints() {
        assert!(SYSTEM_PROMPT.contains("0.8mm"));
        assert!(SYSTEM_PROMPT.contains("315"));
    }

    /// 2026-08-20 追加: system_prompt が iGPU prefill 実用限界を超えないか監視
    ///
    /// iGPU (Apple M3 + Qwen 3.5-4B) の prefill 時間は prompt 長 O(n²) スケーリング
    /// 実測: ~2500 chars で 180s、6081 chars で 300s+ timeout (v0.1.0-beta.1 実測)
    /// 4500 chars 以下なら 3-min timeout に収まる安全域
    ///
    /// Sprint C (2026-08-19) で SHORTCUT section 追加時に 6081 chars まで膨張
    /// させて LLM 経路 B を timeout で機能停止させた事故を再発防止
    #[test]
    fn system_prompt_size_within_igpu_prefill_budget() {
        const MAX_CHARS: usize = 4500;
        assert!(
            SYSTEM_PROMPT.len() <= MAX_CHARS,
            "system_prompt.md is {} chars, exceeds iGPU prefill budget {} chars \
             (>~5000 causes 300s HTTP timeout on iGPU + 3B model, see \
             feedback_text_to_print_system_prompt_size_guard.md)",
            SYSTEM_PROMPT.len(),
            MAX_CHARS
        );
    }

    #[test]
    fn system_prompt_teaches_high_level_shortcuts() {
        // 2026-08-20 追加: SHORTCUT section で 12 archetype + preset 系を LLM に
        // 教えているか確認 (「ペン立て」→ `pen_cup(...)` を思いつく前提)
        assert!(SYSTEM_PROMPT.contains("SHORTCUT"));
        for name in [
            "gridfinity_bin",
            "sticky_note_holder",
            "business_card_holder",
            "pen_cup",
            "phone_stand",
            "coaster",
            "tissue_box_cover",
            "storage_box",
            "wall_hook",
            "skadis_panel",
            "cable_clip",
            "led_channel",
            "card_tray",
            "token_well",
            "wrench_holder",
            "socket_rail",
            "hex_bit_holder",
            "raspi_case",
            "esp32_enclosure",
            "battery_18650_holder",
            "toothbrush_holder",
            "drill_bit_holder",
            "pliers_rack",
            "spice_rack",
            "egg_tray",
            "utensil_caddy",
            "filament_spool_holder",
            "nozzle_holder",
            "build_plate_rack",
            "cutlery_tray",
            "pill_organizer",
            "magnetic_strip",
            "hairdryer_holder",
            "kcup_holder",
            "hex_key_holder",
            "wrap_holder",
            "sock_divider",
            "soap_tray",
            "razor_holder",
            "chopstick_holder",
            "swatch_holder",
            "tp_holder",
            "sd_card_holder",
            "driver_rack",
            "cotton_dispenser",
            "sink_caddy",
            "clamp_rack",
            "dry_box",
            "outdoor_enclosure",
            "jewelry_stand",
            "phone_dock",
            "cutting_board_rack",
            "tape_dispenser",
            "shower_caddy",
            "caliper_holder",
            "bag_clip_org",
            "can_rack",
            "led_hub_box",
            "makeup_organizer",
        ] {
            assert!(
                SYSTEM_PROMPT.contains(name),
                "system_prompt.md must teach LLM about '{name}' SHORTCUT"
            );
        }
    }
}
