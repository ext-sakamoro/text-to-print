//! ライセンス鍵ペア生成ツール
//! 秘密鍵は標準出力、公開鍵は Rust ソース形式で出力

fn main() {
    let issuer = tdvbgaran_core::license::LicenseIssuer::generate();
    let pub_bytes = issuer.public_key_bytes();

    eprintln!("=== License Key Pair Generated ===");
    eprintln!("Store the secret key securely. Never commit it to git.");
    eprintln!();

    // 秘密鍵 (hex)
    let secret_hex = hex::encode(issuer.secret_bytes());
    println!("SECRET_KEY={secret_hex}");
    eprintln!();

    // 公開鍵 (Rust ソース)
    eprint!("const LICENSE_PUBLIC_KEY: [u8; 32] = [");
    for (i, b) in pub_bytes.iter().enumerate() {
        if i > 0 {
            eprint!(", ");
        }
        eprint!("0x{b:02x}");
    }
    eprintln!("];");
}
