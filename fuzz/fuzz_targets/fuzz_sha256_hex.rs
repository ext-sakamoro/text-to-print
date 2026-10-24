//! Fuzz target: sha256_hex (manifest 用 hash) が任意 bytes で panic しないことを検証

#![no_main]

use libfuzzer_sys::fuzz_target;
use text_to_print_core::manifest::sha256_hex;

fuzz_target!(|data: &[u8]| {
    let _ = sha256_hex(data);
});
