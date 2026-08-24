# GO_PUBLIC_CHECKLIST — text-to-print public 化手順

`ext-sakamoro/text-to-print` を private → public に切替える当日の手順集約 実行日の前日〜前週にこの file を通読し、当日は上から順に check していく

**現状 (2026-08-24)**: private 維持、v0.1.0 β release 済 (2026-08-22、7 artifact) audit 実施済 (F、実 secret leak なし、要対処 = `CONTRIBUTING.md` の `SBR` 語彙 1 箇所)

---

## Phase 1: Pre-flight audit (実行日の前日〜前週)

### 1.1 Secret leak grep (tracked file 全体)

```bash
cd ~/text-to-print
git ls-files | xargs grep -lEn "sk_live|sk_test|whsec_|github_pat_|ghp_|Bearer [A-Za-z0-9_-]{20,}|AIza[A-Za-z0-9_-]{20,}|xoxb-|xoxp-|-----BEGIN [A-Z ]+PRIVATE KEY-----" 2>/dev/null
```

- [ ] hit ゼロ or 全て placeholder (`.env.example` / doc の `sk_test_...` / unit test 内の `TEST_SECRET`) と確認済
- [ ] 実 secret を含む file があれば **git filter-repo で履歴消去** (単純 delete では git history に残る)

### 1.2 Private project attribution mix grep

```bash
git ls-files | xargs grep -lEni "SBR|ai-tencho|Toriki|VANSAN|WSC|SPACID|BIPROGY|Secret-Treasure-Chest|CTW|Cover Corp|hololive" 2>/dev/null
```

- [ ] `CONTRIBUTING.md` から `SBR` を「業務案件」等の中立語彙に置換済 (2026-08-24 audit で残 1 件)
- [ ] font binary 等 false positive は無視 OK
- [ ] ~/claude-config/CLAUDE-PROJECT-ISOLATION.md の 7 project 全てに対して同 grep 実施

### 1.3 Cargo.toml meta 確認

```bash
grep -E "description|repository|homepage" Cargo.toml crates/*/Cargo.toml
```

- [ ] `crates/app/Cargo.toml` description = standalone 文面 (旧 SaaS 記述除去済)
- [ ] `repository` = `https://github.com/ext-sakamoro/text-to-print`
- [ ] `homepage` 適切

### 1.4 .gitignore で CLAUDE.md 除外

- [ ] `.gitignore` に `CLAUDE.md` + `.claude/` エントリ存在
- [ ] `git check-ignore CLAUDE.md` が hit する (無出力なら ignored)

### 1.5 README screenshot 全 6 個 file 存在

```bash
ls docs/images/
```

- [ ] `hero.gif`
- [ ] `hero.png`
- [ ] `template-section.png`
- [ ] `customizer.png`
- [ ] `llm-generation.png`
- [ ] `bambu-import.png`
- [ ] `print-result.jpg`

`docs/images/CAPTURE_GUIDE.md` 参照 (1600×1200 / macOS Dark / max 5 MB / GIF ~15 fps) user 側で撮影

### 1.6 LICENSE file

- [ ] `LICENSE` file 存在 (MIT)
- [ ] `Cargo.toml` の `license = "MIT"` と一致

---

## Phase 2: 直前検証 (public 化当日の午前)

### 2.1 CI green 確認

```bash
cd ~/text-to-print
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings
cargo check --target wasm32-unknown-unknown -p text-to-print-worker
```

- [ ] all pass (実測 232+ tests 想定、日付 __________)
- [ ] clippy 0 warnings
- [ ] worker wasm32 build green

### 2.2 GitHub Release 状態

```bash
gh release view v0.1.0
```

- [ ] 公開状態 `published`
- [ ] artifact 7 個揃 (macOS aarch64 + x86_64 / Windows msi + zip / Linux tar.gz + deb + AppImage)

### 2.3 Cloudflare Worker 稼働

```bash
curl -s -w "\nHTTP:%{http_code}\n" https://text-to-print.alicelaw.net/api/presets | head -5
curl -s -o /dev/null -w "HTTP:%{http_code}\n" -X POST https://text-to-print.alicelaw.net/api/share -H "Content-Type: application/json" -d '{}'
```

- [ ] `/api/presets` → 200 + JSON 返却
- [ ] `/api/share` → 400 validation reject (endpoint 稼働 + schema validation 動作)

### 2.4 GitHub Actions billing 状態

- [ ] https://github.com/settings/billing で payment method active
- [ ] spending limit > 0 (0 だと private でも public でも全 runner block)
- [ ] 参考 memory: `feedback_github_actions_billing_blocked_2026_08_11`

### 2.5 external OSS review skill 適用

- [ ] `external-oss-pr-selfreview` skill を通読 (該当なら invoke)
- [ ] README の英語部分校正 (typo / broken link)

---

## Phase 3: Public 化 command 実行

### 3.1 Public 化

```bash
gh repo edit ext-sakamoro/text-to-print --visibility public --accept-visibility-change-consequences
```

- [ ] 上記 command success
- [ ] `gh api repos/ext-sakamoro/text-to-print --jq '.visibility'` → `public` 返却
- [ ] browser で https://github.com/ext-sakamoro/text-to-print を **未認証** で開いて閲覧可能確認

### 3.2 Release URL 疎通 (未認証で)

- [ ] https://github.com/ext-sakamoro/text-to-print/releases/tag/v0.1.0 → 200 (旧 private では 404)
- [ ] artifact DL link を **未認証** で試す → 200

---

## Phase 4: 告知 (Phase 3 後、時間置いて OK)

### 4.1 Ko-fi (sakamoro)

- [ ] `ko-fi.com/sakamoro` の post に v0.1.0 β release 告知 (Release URL + 特徴 3 点)
- [ ] `reference_ko-fi_accounts` 参照 (STC 匿名アカウントと混同禁止)

### 4.2 SNS (X / Bluesky 等)

- [ ] `public-comm-style` skill 準拠 (句点なし / 誇張禁止 / 淡々)
- [ ] 特徴 3 点 (Standalone / LLM 経由 / Bambu 3MF 直接生成)
- [ ] Release URL + hero.gif 添付

### 4.3 ALICE community (該当あれば)

- [ ] Project-ALICE org README に text-to-print reference 追加検討

---

## Phase 5: Post-public monitoring (直後 1-7 日)

### 5.1 Cloudflare Worker 監視

- [ ] `wrangler tail text-to-print` で `/api/share` 到達数 / 400 reject 傾向確認
- [ ] rate limit (`PER_UUID_HOURLY=10` / `PER_IP_HOURLY=100`) の妥当性見直し
- [ ] KV / D1 使用量 (`text-to-print-shares` D1)

### 5.2 GitHub Issue / Discussions

- [ ] 通知 ON (Watch → All Activity)
- [ ] Issue template 準備 (Bug report / Feature request)
- [ ] 対応言語 (日本語 primary、英語 secondary 明記)

### 5.3 build 失敗 report への対応

- 「clone → cargo build 失敗」= ALICE-Bamboo private access なし (README §Build に明記済)
- 「バイナリ起動失敗」= sidecar 起動失敗の可能性 (BYO LLM 経路 or `scripts/build_sidecar.sh` 案内)

---

## Rollback 手順 (万一問題発生時)

### 判断基準

- **軽微** (typo / link 切れ): public のまま修正 commit
- **重大** (secret leak / attribution 誤露出 / severe bug): 即 private 戻し

### Private 戻し

```bash
gh repo edit ext-sakamoro/text-to-print --visibility private --accept-visibility-change-consequences
```

- 24h 以内 = 比較的無害 (fork / star / clone は保持されるが未認証者は再アクセス不可)
- 24h 超過 = 検索 index / cache に残る、DL 済 user には届いてしまう

### Secret leak 発覚時

- [ ] 該当 secret を **即 rotate** (Stripe / GitHub PAT / API key)
- [ ] `git filter-repo` で履歴から除去
- [ ] `git push --force` で GitHub 上を書換 (履歴改変警告あり)
- [ ] GitHub Support にも連絡 (cache / clone 削除依頼)

---

## 参考 memory / skill

- `reference-text-to-print-build-env` — 6 sibling repo + PAT scope + build 罠 13 個
- `project_text_to_print_release_yml_reduction_2026_08_11` — release automation 現状 + 5 罠 catalog
- `feedback_github_actions_billing_blocked_2026_08_11` — Actions billing 罠
- `feedback_cargo_wix_release_yml_pitfalls` — Windows msi build 罠
- `success_text_to_print_byo_llm_2026_08_23` — BYO LLM 経路 (β user 導線)
- `reference_ko-fi_accounts` — Ko-fi 2 保有の切分け (実名 vs 匿名 STC 混同禁止)
- `external-oss-pr-selfreview` skill — 外部 OSS 対応の self-review
- `public-comm-style` skill — 公共物 writing (句点なし / 誇張禁止)
- `~/claude-config/CLAUDE-PROJECT-ISOLATION.md` — 7 private project attribution 除外リスト
