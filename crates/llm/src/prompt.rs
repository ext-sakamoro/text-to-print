/// LOL DSL 生成用のシステムプロンプト
pub const SYSTEM_PROMPT: &str = include_str!("system_prompt.md");

/// User prompt に含まれる cm / m / inch / inches を mm へ deterministic に変換
///
/// LOL DSL は mm 単位固定 (system_prompt 「mm units」記載)、しかし user は自然に
/// 「直径5cm」「1インチ」等の可読単位で prompt を書くため、LLM に unit conversion を
/// 任せると 2-3B iGPU model 帯で誤解釈が発生する
///
/// # 実測失敗例 (2026-09-12 MiniCPM5-2B Q4_K_M、Apple M3 CPU --hybrid)
///
/// User: 「直径5cm、高さ10cmで取手のついたマグカップ」→ MiniCPM5 が think block 内で
/// 「Let's make sure the units are in mm. The DSL says 'mm units'. So 5 and 10 are
/// in mm.」と誤決定 → 最終出力 `pen_cup(5, 10)` = **5mm dia × 10mm 高さ** = ミニ玩具
/// (正解は `pen_cup(50, 100)` = 5cm dia × 10cm 高さ)
///
/// # 変換 rule
///
/// | 入力 unit | 変換式 | 例 |
/// |----------|--------|-----|
/// | `cm` | × 10 | `5cm` → `50mm` |
/// | `m` (単独、`mm`/`mile` 等の prefix でない) | × 1000 | `1m` → `1000mm` |
/// | `mm` | unchanged | `5mm` → `5mm` |
/// | `inch` / `inches` | × 25.4 | `1inch` → `25.4mm` |
///
/// # 数値 formatting
///
/// - 整数結果 (`5cm` → `50mm`): 整数出力
/// - 小数結果 (`1inch` → `25.4mm`): 1 桁小数出力
/// - unit なし数字 (「5000 users」等): 変更なし
///
/// # Bambu H2D bed 制約
///
/// Bambu H2D bed size は 315×315×320mm (`system_prompt.md` 参照)、user の「1m マグカップ」
/// のような bed 超過値は変換後 `1000mm` になる (現状は変換のみ、警告は将来 work)
#[must_use]
pub fn normalize_units(input: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // 数字 (整数 or 小数) + 任意の空白 + 単位 (最長 match 順で alternation)、
        // 末尾は **ASCII-only word boundary** `(?-u:\b)` を使う
        //
        // ASCII-only boundary が必要な理由: Rust regex crate の `\b` は default で
        // Unicode word char 認識、日本語 hiragana (例: 「10cmで」の 「で」) が word
        // char 扱いになり `m\b` が boundary 認識せず match 失敗する事案 (2026-09-12
        // 実測: `直径5cm、高さ10cmで` → `直径50mm、高さ10cmで` = 後段 10cm 変換漏れ)
        // `(?-u:\b)` は ASCII word char (`[A-Za-z0-9_]`) のみで boundary 判定するため
        // 「で」は非 word 扱いで boundary あり → match 成功する
        Regex::new(r"(\d+(?:\.\d+)?)\s*(cm|inches|inch|mm|m)(?-u:\b)").expect("static regex")
    });
    re.replace_all(input, |caps: &regex::Captures| {
        let num_str = caps.get(1).map_or("", |m| m.as_str());
        let unit = caps.get(2).map_or("", |m| m.as_str());
        let Ok(num) = num_str.parse::<f32>() else {
            return caps
                .get(0)
                .map_or(String::new(), |m| m.as_str().to_string());
        };
        let (mm, changed) = match unit {
            "cm" => (num * 10.0, true),
            "m" => (num * 1000.0, true),
            "inch" | "inches" => (num * 25.4, true),
            "mm" => (num, false),
            _ => (num, false),
        };
        if !changed {
            // preserve original "Nmm" format (avoid `5.0mm` → `5mm` rewrite)
            return caps
                .get(0)
                .map_or(String::new(), |m| m.as_str().to_string());
        }
        // 整数 mm は整数表記、小数は 1 桁 (0.1mm 精度は 3D print で十分)
        if (mm.fract()).abs() < f32::EPSILON {
            format!("{}mm", mm as i64)
        } else {
            format!("{mm:.1}mm")
        }
    })
    .into_owned()
}

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
             (>~5000 causes 300s HTTP timeout on iGPU + 3B model)",
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

    // ─── normalize_units tests ─────────────────────────────────────

    #[test]
    fn normalize_units_cm_to_mm_integer() {
        assert_eq!(normalize_units("5cm"), "50mm");
        assert_eq!(normalize_units("12cm"), "120mm");
    }

    #[test]
    fn normalize_units_cm_to_mm_decimal() {
        assert_eq!(normalize_units("2.5cm"), "25mm");
        assert_eq!(normalize_units("1.7cm"), "17mm");
    }

    #[test]
    fn normalize_units_mm_unchanged() {
        assert_eq!(normalize_units("5mm"), "5mm");
        assert_eq!(normalize_units("2.5mm"), "2.5mm");
    }

    #[test]
    fn normalize_units_meter_to_mm() {
        assert_eq!(normalize_units("1m"), "1000mm");
        assert_eq!(normalize_units("0.5m"), "500mm");
    }

    #[test]
    fn normalize_units_inch_to_mm() {
        assert_eq!(normalize_units("1inch"), "25.4mm");
        assert_eq!(normalize_units("2inches"), "50.8mm");
    }

    #[test]
    fn normalize_units_whitespace_tolerated() {
        assert_eq!(normalize_units("5 cm"), "50mm");
        assert_eq!(normalize_units("1 inch"), "25.4mm");
    }

    #[test]
    fn normalize_units_word_boundary_avoids_false_positives() {
        // `miles` の `m` は word char follow で boundary なし → match しない
        assert_eq!(normalize_units("5 miles"), "5 miles");
        // `million` 同上
        assert_eq!(normalize_units("100 million"), "100 million");
        // `centimeter` の `cm` は c の直前に digit ない → NOT match
        assert_eq!(normalize_units("centimeter"), "centimeter");
    }

    #[test]
    fn normalize_units_japanese_mug_prompt() {
        // 2026-09-12 事案: MiniCPM5-2B が「直径5cm、高さ10cm」を mm と誤解釈した
        // pattern を deterministic に矯正
        let out = normalize_units("直径5cm、高さ10cmで取手のついたマグカップ");
        assert_eq!(out, "直径50mm、高さ100mmで取手のついたマグカップ");
    }

    #[test]
    fn normalize_units_mixed_units() {
        assert_eq!(
            normalize_units("底面 5cm × 10cm、厚み 3mm、脚 1inch"),
            "底面 50mm × 100mm、厚み 3mm、脚 25.4mm"
        );
    }

    #[test]
    fn normalize_units_no_units_unchanged() {
        assert_eq!(normalize_units("Total 5000 users"), "Total 5000 users");
        assert_eq!(normalize_units("プレゼント"), "プレゼント");
    }

    #[test]
    fn normalize_units_bed_overshoot_still_converts() {
        // Bambu H2D bed 315mm を超過するが warning は将来 work (現状は変換のみ)
        assert_eq!(normalize_units("1m の巨大マグ"), "1000mm の巨大マグ");
    }

    /// 2026-09-12 追加: 「取手付きマグカップ」canonical example が system_prompt に
    /// あるか確認 pen_cup shortcut は取手なし単純ペン立てなので、mug に必要な
    /// 取手 (torus handle) を union するパターンを model が真似できるように
    /// concrete example を保持する目的
    ///
    /// 事案背景: MiniCPM5-2B-Q4_K_M で unit normalize 後の「Ø50 h100 取手つき」
    /// prompt を投げた際、compound reasoning (cylinder body + handle attachment)
    /// で circular loop に陥り 3000 token budget 内に最終 DSL 未 emit 事案
    /// (2026-09-12、[[success_alice_llm_llama_preq_matvec_oob_fix_minicpm5]] 続編)
    #[test]
    fn system_prompt_teaches_mug_pattern() {
        assert!(
            SYSTEM_PROMPT.contains("マグカップ"),
            "system_prompt.md must teach 'マグカップ' canonical example"
        );
        // pen_cup body + torus handle union pattern (2 primitive combo)
        assert!(
            SYSTEM_PROMPT.contains("pen_cup(50,100)"),
            "system_prompt.md must show pen_cup body dimensions example"
        );
        assert!(
            SYSTEM_PROMPT.contains("torus(15,5)"),
            "system_prompt.md must show torus handle example"
        );
    }
}
