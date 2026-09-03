//! Fuzz target: AliceManifest の JSON deserialize が任意 input で panic しないことを検証
//!
//! .3mf embed の manifest 経由で不正 JSON が入り得るため

#![no_main]

use libfuzzer_sys::fuzz_target;
use text_to_print_core::manifest::AliceManifest;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let _ = serde_json::from_str::<AliceManifest>(s);
});
