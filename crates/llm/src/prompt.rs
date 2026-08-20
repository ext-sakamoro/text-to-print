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
        ] {
            assert!(
                SYSTEM_PROMPT.contains(name),
                "system_prompt.md must teach LLM about '{name}' SHORTCUT"
            );
        }
    }
}
