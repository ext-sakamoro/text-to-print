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
}
