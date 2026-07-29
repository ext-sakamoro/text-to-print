//! Safety-violation → LLM fix-prompt template (Stage 8 T8.1)
//!
//! When the pipeline finishes and reports safety violations
//! (`alice_bamboo::safety::SafetyReport` messages / warp category), the
//! caller feeds those violations back into the LLM via a retry so the
//! next LOL DSL suggestion avoids the same failure mode
//!
//! This module maps a stable, structured [`SafetyViolationKind`] into a
//! natural-language fix instruction the LLM can act on
//! [`fix_prompt_for_violations`] combines multiple violations into one
//! composite instruction and returns an empty string when the input is
//! empty, so callers can concatenate the output onto the original prompt
//! unconditionally

use serde::{Deserialize, Serialize};

/// Discrete safety violation categories surfaced by the pipeline
///
/// The variants are ordered by remediation priority (warp is the most
/// print-fatal failure mode, thin walls are close behind, thermal /
/// overhang are informational unless they exceed strict thresholds)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SafetyViolationKind {
    /// Warp risk classified as `Critical` (>75 % risk score)
    WarpCritical,
    /// Warp risk classified as `High` (50-75 %)
    WarpHigh,
    /// Warp risk classified as `Medium` (25-50 %)
    WarpMedium,
    /// Thermal stress with a factor of safety below 2.0
    ThermalUnsafe,
    /// Operating temperature is within 10 °C of the glass transition
    ThermalNearGlassTransition,
    /// Beam load case reports a factor of safety below the material minimum
    BeamOverloaded,
    /// Overhang ratio exceeds the 30 % informal budget
    OverhangExcessive,
}

impl SafetyViolationKind {
    /// Human-readable label used in prompts and logs
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::WarpCritical => "warp risk (critical)",
            Self::WarpHigh => "warp risk (high)",
            Self::WarpMedium => "warp risk (medium)",
            Self::ThermalUnsafe => "thermal stress (FoS < 2)",
            Self::ThermalNearGlassTransition => "operating temp near Tg",
            Self::BeamOverloaded => "beam bending overload",
            Self::OverhangExcessive => "overhang exceeds budget",
        }
    }

    /// Per-violation remediation directive for the LLM
    #[must_use]
    pub const fn fix_directive(self) -> &'static str {
        match self {
            Self::WarpCritical | Self::WarpHigh => {
                "Reduce the maximum XY footprint (under 200 mm on the long side) or add fillets \
                 / brims to redistribute cool-down stress"
            }
            Self::WarpMedium => {
                "Consider reducing the XY footprint slightly or adding a small brim"
            }
            Self::ThermalUnsafe => {
                "Increase the load-bearing wall thickness or switch to a higher-Tg material \
                 (PETG / ABS instead of PLA)"
            }
            Self::ThermalNearGlassTransition => {
                "Keep the operating temperature below 55 °C or avoid persistent load on this part"
            }
            Self::BeamOverloaded => {
                "Thicken the load-bearing section, shorten the moment arm, or add reinforcing \
                 gussets so the bending stress falls below the material yield"
            }
            Self::OverhangExcessive => {
                "Reduce steep overhangs (>45°) or add chamfers / fillets so support material is \
                 minimised — target < 30 % overhang area"
            }
        }
    }

    /// Attempt to infer the violation kind from an existing
    /// `alice_bamboo::safety::SafetyReport` message string
    ///
    /// This is a best-effort keyword classifier used to preserve backward
    /// compatibility with callers that only have text messages
    #[must_use]
    pub fn from_message(msg: &str) -> Option<Self> {
        let lower = msg.to_lowercase();
        if lower.contains("critical") && lower.contains("warp") {
            Some(Self::WarpCritical)
        } else if lower.contains("high") && lower.contains("warp") {
            Some(Self::WarpHigh)
        } else if lower.contains("medium") && lower.contains("warp") {
            Some(Self::WarpMedium)
        } else if lower.contains("near tg") || lower.contains("glass transition") {
            Some(Self::ThermalNearGlassTransition)
        } else if lower.contains("thermal") && lower.contains("fos") {
            Some(Self::ThermalUnsafe)
        } else if lower.contains("beam") || lower.contains("bending") {
            Some(Self::BeamOverloaded)
        } else if lower.contains("overhang") {
            Some(Self::OverhangExcessive)
        } else {
            None
        }
    }
}

/// Build a composite fix-prompt suffix from a set of safety violations
///
/// Returns an empty `String` when `kinds` is empty so callers can always
/// concatenate the result onto the original user prompt Otherwise the
/// output takes the form:
///
/// ```text
/// Your previous LOL DSL had the following safety issues:
///   - {label}: {fix_directive}
///   - {label}: {fix_directive}
/// Please generate a revised LOL DSL that resolves all of the above
/// ```
#[must_use]
pub fn fix_prompt_for_violations(kinds: &[SafetyViolationKind]) -> String {
    if kinds.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\n\nYour previous LOL DSL had the following safety issues (per the alice-physics \
         verification pipeline):\n",
    );
    for k in kinds {
        out.push_str("  - ");
        out.push_str(k.label());
        out.push_str(": ");
        out.push_str(k.fix_directive());
        out.push('\n');
    }
    out.push_str("Please generate a revised LOL DSL that resolves all of the above.");
    out
}

/// Convenience: build a fix prompt directly from a set of `SafetyReport`
/// message strings (routed through [`SafetyViolationKind::from_message`])
///
/// Messages that cannot be classified are silently dropped so the fix
/// prompt only contains actionable guidance
#[must_use]
pub fn fix_prompt_from_messages(messages: &[String]) -> String {
    let kinds: Vec<SafetyViolationKind> = messages
        .iter()
        .filter_map(|m| SafetyViolationKind::from_message(m))
        .collect();
    fix_prompt_for_violations(&kinds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_violations_return_empty_string() {
        assert_eq!(fix_prompt_for_violations(&[]), "");
    }

    #[test]
    fn single_violation_contains_label_and_directive() {
        let out = fix_prompt_for_violations(&[SafetyViolationKind::WarpCritical]);
        assert!(out.contains("warp risk (critical)"));
        assert!(out.contains("XY footprint"));
        assert!(out.contains("revised LOL DSL"));
    }

    #[test]
    fn multiple_violations_all_included() {
        let out = fix_prompt_for_violations(&[
            SafetyViolationKind::WarpCritical,
            SafetyViolationKind::BeamOverloaded,
        ]);
        assert!(out.contains("warp risk (critical)"));
        assert!(out.contains("beam bending overload"));
    }

    #[test]
    fn message_classifier_maps_known_strings() {
        assert_eq!(
            SafetyViolationKind::from_message("Warp risk Critical: reduce XY"),
            Some(SafetyViolationKind::WarpCritical)
        );
        assert_eq!(
            SafetyViolationKind::from_message("Operating temp near Tg 55 °C"),
            Some(SafetyViolationKind::ThermalNearGlassTransition)
        );
        assert_eq!(
            SafetyViolationKind::from_message("Beam FoS 0.8 below minimum"),
            Some(SafetyViolationKind::BeamOverloaded)
        );
        assert_eq!(
            SafetyViolationKind::from_message("something unrelated"),
            None
        );
    }

    #[test]
    fn fix_prompt_from_messages_filters_unknown() {
        let messages = vec![
            "Warp risk High: shrink footprint".to_string(),
            "unknown noise message".to_string(),
            "Beam FoS too low".to_string(),
        ];
        let out = fix_prompt_from_messages(&messages);
        assert!(out.contains("warp risk (high)"));
        assert!(out.contains("beam bending overload"));
        assert!(!out.contains("unknown noise"));
    }

    #[test]
    fn all_kinds_have_nonempty_label_and_directive() {
        for k in [
            SafetyViolationKind::WarpCritical,
            SafetyViolationKind::WarpHigh,
            SafetyViolationKind::WarpMedium,
            SafetyViolationKind::ThermalUnsafe,
            SafetyViolationKind::ThermalNearGlassTransition,
            SafetyViolationKind::BeamOverloaded,
            SafetyViolationKind::OverhangExcessive,
        ] {
            assert!(!k.label().is_empty());
            assert!(!k.fix_directive().is_empty());
        }
    }
}
