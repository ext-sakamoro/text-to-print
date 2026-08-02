//! LOL DSL GBNF grammar embedded for constrained decoding (Stage 3-C.11)
//!
//! `LOL_GBNF` is a verbatim copy of the authoritative grammar shipped with
//! [ALICE-LOL] (`skills/lol-sdf/references/lol.gbnf`) It covers the 124
//! constructs recognised by `alice_lol::runtime_parser::parse_lol` and
//! enforces (a) known construct names (b) argument shape (number vs child
//! count, comma placement, balanced parens) (c) permitted whitespace and
//! `//` line comments
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

/// Verbatim GBNF grammar for the LOL DSL Copy of the ALICE-LOL skill's
/// canonical grammar at `skills/lol-sdf/references/lol.gbnf` (kept in
/// sync manually; check `git log` on the source file when updating)
pub const LOL_GBNF: &str = include_str!("lol.gbnf");

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
}
