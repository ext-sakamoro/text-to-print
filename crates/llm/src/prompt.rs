//! system prompt と単位の正規化は `text_to_print_core::prompt` にある (iOS の bridge と共有) ここは再公開だけ

pub use text_to_print_core::prompt::{SYSTEM_PROMPT, normalize_units};

#[cfg(test)]
mod tests {
    use super::{SYSTEM_PROMPT, normalize_units};

    #[test]
    fn the_system_prompt_is_the_same_bytes_as_core() {
        assert_eq!(
            SYSTEM_PROMPT.as_bytes(),
            text_to_print_core::prompt::SYSTEM_PROMPT.as_bytes()
        );
        assert_eq!(
            SYSTEM_PROMPT.as_bytes(),
            include_bytes!("../../core/src/system_prompt.md").as_slice()
        );
    }

    #[test]
    fn normalize_units_agrees_with_core() {
        let mut compared = 0;
        for input in [
            "直径5cm、高さ10cmで",
            "1inch のねじ",
            "2.5cm",
            "1m のマグカップ",
            "5mm",
            "5000 users",
        ] {
            assert_eq!(
                normalize_units(input),
                text_to_print_core::prompt::normalize_units(input),
                "{input}"
            );
            compared += 1;
        }
        assert_eq!(compared, 6);
    }
}
