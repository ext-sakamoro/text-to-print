//! Fuzz target: LicenseKey::from_base64 が任意 str 入力で panic しないことを検証
//!
//! canonical CI template [[reference_alice_ci_canonical_template]] 準拠
//! ライセンス verify は本ソフトの商用 gate、malformed input で panic すると DoS

#![no_main]

use libfuzzer_sys::fuzz_target;
use text_to_print_core::license::LicenseKey;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = LicenseKey::from_base64(s);
});
