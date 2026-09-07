# Contributing to text-to-print

自然言語 → LOL DSL → Bambu Lab 3MF を生成する Rust スタンドアローン desktop app です contribution を歓迎します

## 開発環境セットアップ

### 必須ツール

- **Rust stable** (edition 2024 対応、rustc 1.85+)
- **cargo** (Rust 標準)
- **Xcode Command Line Tools** (macOS のみ、`xcode-select --install`)

### ALICE-* sibling repositories

text-to-print は `../ALICE-*` にある sibling repository を path 依存として参照します 以下 6 つを text-to-print と同じ親 dir に clone してください:

```bash
cd ~/
git clone https://github.com/Project-ALICE/ALICE-SDF.git       # public
git clone https://github.com/Project-ALICE/ALICE-LOL.git       # public
git clone https://github.com/Project-ALICE/ALICE-View.git      # public
git clone https://github.com/Project-ALICE/ALICE-LLM.git       # public
git clone https://github.com/ext-sakamoro/ALICE-Bamboo.git     # private (要アクセス権)
git clone https://github.com/ext-sakamoro/ALICE-Physics.git    # private (要アクセス権)
git clone https://github.com/ext-sakamoro/text-to-print.git
```

Private repo にアクセス権がない場合は、access request を issue に投稿してください

### ビルド + 動作確認

```bash
cd ~/text-to-print
cargo build --workspace
cargo test --workspace --lib
cargo run --release --bin text-to-print
```

初回ビルドは 3-10 分かかります (alice-* 依存の compile)

## 品質ゲート

**PR は以下 4 つが green である必要があります:**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
cargo doc --workspace --no-deps
```

CI (`.github/workflows/ci.yml`) は 3-OS (Ubuntu / macOS / Windows) matrix でこれを実行します

## commit convention

- Subject: **imperative form** (`feat: add ...`, `fix: handle ...`)
- 日本語 subject 可 (例 `feat: 4色 export UI 追加`)
- **署名は不要** (自動署名系を追加しない)
- 句点 (`。`) は使わず改行 or 半角スペースで区切る

commit prefix:
- `feat:` — 新機能
- `fix:` — bug 修正
- `refactor:` — 挙動を変えない構造修正
- `test:` — テスト追加
- `docs:` — ドキュメント
- `ci:` — CI 設定
- `chore:` — 依存更新 / 整形など

## PR 手順

1. `main` から feature branch を作成 (`git switch -c feat/my-feature`)
2. 変更 + tests + docs
3. 品質ゲート全 pass 確認
4. **GAP-B 等 dead code / dead field を残さない** (実装済 API を UI/呼び出し側から実接続する)
5. PR 作成 (title は日本語可)
6. CI 3-OS matrix green + reviewer approval で merge

## 実装上の規約

### dead code / dead field 禁止

バックエンド API を実装したら **必ず呼び出し側 (UI / async task) を同 PR で接続** することが原則です 呼び出し側が未接続な状態で backend を merge することは `GAP-11` / `GAP-12` のような後追い修正を生むため避けてください

### コメントの書き方

- 「なぜ」を書く (「何」は well-named identifier が既に伝える)
- 過去 issue や PR # を書かない (rot するため — commit message や PR description に書く)

### 依存追加ポリシー

- 新規依存は workspace deps に追加 (`Cargo.toml` の `[workspace.dependencies]`)
- default features は最小化 (`default-features = false, features = [...]`)
- publish=false crate は path deps で参照
- 動機を PR で明示

## テスト観点

### unit tests

- `#[cfg(test)] mod tests { ... }` inline
- 依存が実物 (LLM / GPU 等) の場合は mock / stub を作る
- 目安: 主要 API + edge case + error path

### integration tests (crate `tests/`)

- `crates/*/tests/*.rs` に置く
- E2E path (LLM → LOL → 3MF) を verify する場合、LLM 部分は hard-coded mock response

## 領域別担当

主な作業対象:

- `crates/core/` — pipeline / DB / manifest / license
- `crates/llm/` — LLM backend / retry / fix_prompt / sidecar
- `crates/network/` — P2P / share payload / SharePayload / VCS
- `crates/app/` — egui GUI / state / prompt UI / settings

## セキュリティ / シークレット

- API key / private key / license private key を **絶対に commit しない**
- `.gitignore` を必ず確認
- CI で使う場合は GitHub Secrets (`docs/RELEASE.md` §Secrets 参照)

## Release process

release automation は `docs/RELEASE.md` を参照

`v*` tag push で `.github/workflows/release.yml` が自動起動、3-OS matrix build + macOS codesign + notarize + GitHub Releases 公開まで実行されます

## 質問 / discussion

- **バグ**: GitHub Issues (`bug` label)
- **機能要望**: GitHub Issues (`feat` label)
- **議論**: GitHub Discussions

## ライセンス

MIT License 詳細は `LICENSE` 参照

---

Happy hacking
