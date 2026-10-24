//! LOL DSL GBNF grammar for constrained decoding (Stage 3-C.11)
//!
//! `LOL_GBNF` is the authoritative grammar shipped with [ALICE-LOL],
//! reached through `alice_bamboo::LOL_GBNF` (a re-export of
//! `alice_lol::LOL_GBNF`, feature-free `include_str!`) There is no local
//! copy any more: until 2026-09-14 this crate embedded its own
//! `lol.gbnf`, which drifted 199 lines behind ALICE-LOL (comment rule,
//! whitespace rule, `program(...)` Intent wrapper) The grammar covers the
//! 124 SDF constructs recognised by `alice_lol::runtime_parser::parse_lol`
//! plus the Phase 3 Intent wrapper, and enforces (a) known construct
//! names (b) argument shape (c) at most one whitespace char between
//! tokens, no `//` comments (stricter than the runtime parser on purpose)
//!
//! What the grammar does *not* enforce:
//! - Exact per-construct arity (bucketed by category)
//! - Numeric ranges / semantic validity
//! - `proc_macro`-only `{expr}` interpolation
//!
//! The pipeline still runs [`crate::backend::generate_with_retry`] +
//! `pipeline::safety_check_lol` afterwards to catch semantic issues; the
//! GBNF gate only makes the LOL DSL itself well-formed
//!
//! [ALICE-LOL]: https://github.com/Project-ALICE/ALICE-LOL

use anyhow::{Context, Result};

/// The ALICE-LOL canonical GBNF grammar (same bytes as `alice_lol::LOL_GBNF`)
pub use alice_bamboo::LOL_GBNF;

/// Parse [`LOL_GBNF`] into a runtime-usable `Grammar` Cheap enough
/// (~microseconds) that callers can invoke it per request
///
/// # Errors
///
/// [`alice_llm::grammar::GbnfError`] wrapped in `anyhow::Error` on parse
/// failure — this only fires if the grammar file gets edited into an
/// invalid state
pub fn parse_lol_grammar() -> Result<alice_llm::grammar::Grammar> {
    alice_llm::grammar::parse_gbnf(LOL_GBNF).with_context(|| "parse embedded lol.gbnf")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lol_gbnf_is_non_empty() {
        assert!(!LOL_GBNF.is_empty());
        assert!(LOL_GBNF.contains("root"));
    }

    #[test]
    fn lol_grammar_parses_cleanly() {
        let grammar = parse_lol_grammar().expect("embedded lol.gbnf must be valid GBNF");
        assert!(!grammar.rules.is_empty());
        assert_eq!(grammar.root, "root");
    }

    #[test]
    fn lol_grammar_starts_a_working_fsm() {
        let grammar = parse_lol_grammar().unwrap();
        let fsm = alice_llm::grammar::Fsm::start(&grammar).expect("root has alternatives");
        // `sphere(1.0)` is a canonical minimal LOL expression; the FSM
        // should accept its opening char
        assert!(fsm.accepts('s') || fsm.accepts(' ') || fsm.accepts('\n'));
    }

    #[test]
    fn lol_gbnf_includes_high_level_primitives() {
        // 2026-08-20 追加: 12 archetype 全部 + wall_hook / drawer / shelf_divider /
        // SKADIS preset 系が GBNF に登録されているか (retention gate、future
        // refactor で誤って削除するのを防ぐ)
        let must_have = [
            // 2f primitives
            "shopping_cart_coin",
            "pen_cup",
            "coaster",
            "cable_clip",
            "led_channel",
            // 3f primitives
            "gridfinity_bin",
            "sticky_note_holder",
            "business_card_holder",
            "phone_stand",
            "headphone_holder",
            "under_desk_mount",
            "desk_shelf",
            "monitor_riser",
            "tissue_box_cover",
            "storage_box",
            "skadis_panel",
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
            // 7f primitives
            "gridfinity_bin_ex",
            // no-arg primitives
            "wall_hook",
            "drawer_organizer",
            "shelf_divider",
            "skadis_hook_l",
            "skadis_hook_j",
            "skadis_hook_s",
            "skadis_container",
            "skadis_clip",
            "skadis_shelf",
            "skadis_elastic_cord",
        ];
        for name in must_have {
            assert!(
                LOL_GBNF.contains(&format!("\"{name}\"")),
                "LOL_GBNF must include high-level primitive: {name}"
            );
        }
    }
}
