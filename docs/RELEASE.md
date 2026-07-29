# Release Process

text-to-print の release automation 手順 (Stage 10 T10.2 + T10.3 対応)

`.github/workflows/release.yml` は `v*` tag push でトリガーされ、以下を実行する:

1. 4 target ビルド (aarch64/x86_64 macOS, x86_64 Linux, x86_64 Windows)
2. macOS binary に Developer ID Application 証明書で codesign
3. Apple notarytool で notarize (App Store Connect API key 経由)
4. staple + tar.gz / zip アーカイブ
5. GitHub Releases に upload (auto-generate release notes)

## 事前セットアップ (1 回だけ)

### 1. App Store Connect API Key 作成 + `.p8` 取得

1. [appstoreconnect.apple.com/access/integrations/api](https://appstoreconnect.apple.com/access/integrations/api) にアクセス
2. `+` で新規 key 作成 (name 任意、role は `Developer` 推奨)
3. **`.p8` file を DL (1 回のみ、再 DL 不可)** → `~/.asc/AuthKey_XXXXXX.p8` へ移動 + `chmod 600`
4. **Key ID** (10 char) と **Issuer ID** (UUID) を控える

### 2. Developer ID Application 証明書取得

macOS の外部配布用コードサイニング証明書 (App Store 外配布に必須):

**方法 A: Xcode 経由 (推奨)**
1. Xcode > Settings > Accounts > (Apple ID) > Manage Certificates...
2. `+` → **Developer ID Application** を選択
3. 証明書が Keychain Access に自動で追加される

**方法 B: Apple Developer portal 経由**
1. [developer.apple.com/account/resources/certificates](https://developer.apple.com/account/resources/certificates) にアクセス
2. `+` で新規 → **Developer ID Application** を選択
3. CSR (Certificate Signing Request) を Keychain Access で作成:
   - Keychain Access > 証明書アシスタント > 認証局に証明書を要求
   - "ディスクに保存" を選択、CSR file を保存
4. Apple Developer portal で CSR upload → 証明書 DL
5. `.cer` file をダブルクリックで Keychain Access に import

### 3. `.p12` export (CI 用に certificate + private key を bundle)

1. Keychain Access で `Developer ID Application: <your name>` を選択
2. 右クリック → "書き出す (Export)"
3. Format = `Personal Information Exchange (.p12)`
4. **強い password を設定** (後で `DEVELOPER_ID_APPLICATION_CERT_PASSWORD` に登録)
5. `~/dev-id-application.p12` に保存

### 4. GitHub Secrets 登録

repository の **Settings > Secrets and variables > Actions** で以下を New repository secret として追加:

| Secret 名 | 値の作り方 |
|--|--|
| `ALICE_ECO_TOKEN` | ext-sakamoro/ALICE-Bamboo + ALICE-Physics に `contents:read` 権限を持つ PAT ([Fine-grained tokens](https://github.com/settings/tokens?type=beta) で作成) |
| `APPLE_P8_B64` | `base64 -i ~/.asc/AuthKey_XXXXXX.p8` の出力 (macOS: `base64 -i FILE`、Linux: `base64 -w0 FILE`) |
| `APPLE_KEY_ID` | Key ID (例 `4P2YQS5RAT`) |
| `APPLE_ISSUER_ID` | Issuer ID (UUID) |
| `DEVELOPER_ID_APPLICATION_CERT_B64` | `base64 -i ~/dev-id-application.p12` の出力 |
| `DEVELOPER_ID_APPLICATION_CERT_PASSWORD` | step 3 で設定した `.p12` password |

登録後、`.p12` file は削除してよい (Keychain には残る、`.p8` file はローカル運用で残す)

### 5. Local keychain 登録 (Mac ローカル運用)

CI 用 secrets とは別に、Mac ローカルで手動 release / test build 可能に:

```bash
# asc CLI (App Store Connect API 汎用)
asc auth login \
  --name "text-to-print" \
  --key-id "4P2YQS5RAT" \
  --issuer-id "675c2bd7-6b83-4e52-a830-06f4d386081e" \
  --private-key ~/.asc/AuthKey_4P2YQS5RAT.p8

# notarytool (Xcode 標準、notarize 専用)
xcrun notarytool store-credentials "text-to-print-notarize" \
  --key ~/.asc/AuthKey_4P2YQS5RAT.p8 \
  --key-id "4P2YQS5RAT" \
  --issuer "675c2bd7-6b83-4e52-a830-06f4d386081e"
```

validation:

```bash
asc auth status --validate   # "validation":"works" が返れば OK
xcrun notarytool history --keychain-profile "text-to-print-notarize"
```

## Release 手順

version bump 後 tag を push すれば workflow が自動起動する

```bash
# 1. Cargo.toml の version を上げる (workspace.package.version)
vim Cargo.toml

# 2. commit + tag + push
git add Cargo.toml
git commit -m "chore: release v0.1.1"
git tag v0.1.1
git push origin main
git push origin v0.1.1
```

workflow 進行状況は [Actions タブ](https://github.com/ext-sakamoro/text-to-print/actions) で追跡

## dry run (secrets 動作確認だけ)

release upload なしで build + sign + notarize だけ実行:

1. Actions タブ > Release workflow > "Run workflow"
2. `dry_run` を `true` にして dispatch
3. artifact tab から .tar.gz / .zip を DL して確認

## トラブルシューティング

### `Developer ID Application identity not found in keychain`

`.p12` に private key が含まれていない、または certificate と private key が pair でない
→ Keychain Access で証明書を展開して private key が付いているか確認
→ 無ければ Xcode 経由で新規に作り直す

### notarytool submit で `Invalid credentials`

`.p8` file の Key ID / Issuer ID が secrets の値と mismatch
→ App Store Connect で確認、`APPLE_KEY_ID` は `.p8` file 名 (`AuthKey_<KEY_ID>.p8`) と一致することを確認

### notarize `The signature is invalid`

`.p12` の証明書が Developer ID Application 以外 (例: Mac Development)
→ Keychain Access で `Developer ID Application` prefix の証明書を再 export

### ALICE-* private repo checkout で `Repository not found`

`ALICE_ECO_TOKEN` の scope が不足、または期限切れ
→ [Fine-grained tokens](https://github.com/settings/tokens?type=beta) で再作成、`contents:read` を `ext-sakamoro/ALICE-Bamboo` と `ext-sakamoro/ALICE-Physics` に付与

## Windows Authenticode 署名 (future)

`.msi` は cargo-wix で生成されるが、Windows Defender SmartScreen 回避には
Authenticode 署名が必要 追加手順:

1. DigiCert / SSL.com / GlobalSign から EV Code Signing Certificate を取得
2. `.pfx` を base64 化して `WINDOWS_CERT_B64` + `WINDOWS_CERT_PASSWORD` secret 追加
3. `.github/workflows/release.yml` の "Build .msi installer" step 直後に signtool step 追加:
   ```powershell
   $cert = [Convert]::FromBase64String($env:CERT_B64)
   [IO.File]::WriteAllBytes("$env:RUNNER_TEMP\\code.pfx", $cert)
   & 'C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\signtool.exe' `
     sign /f "$env:RUNNER_TEMP\\code.pfx" /p $env:CERT_PASSWORD `
     /tr http://timestamp.digicert.com /td sha256 /fd sha256 `
     text-to-print-*.msi
   ```

## 関連 issue

- **#31** T10.2: GitHub Actions release workflow (この doc の対象)
- **#32** T10.3: macOS code signing + notarize (この doc の対象)
- **#33** T10.4: Windows .msi packaging (この doc の対象、Authenticode 署名は follow-up)
- **#34** T10.5: Linux .AppImage or .deb packaging (この doc の対象)
- **#30** T10.1: CI 3-OS matrix (別 workflow `ci.yml` で対応済)
- **#40** CI cargo audit (別 workflow `ci.yml` で対応済)
