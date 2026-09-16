# Changelog

本 file は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) 形式に準拠し、
バージョン管理は [Semantic Versioning](https://semver.org/lang/ja/) に従う

## [Unreleased]

### Security
- rustls 0.23.43 → 0.23.45 (RUSTSEC-2026-0285、TLS 1.3 handshake message encryption level 境界) `cargo update -p rustls`
- `deny.toml` を real sibling 依存木で green に: ALICE-* sibling 4 crate (alice-physics / alice-llm AGPL、alice-bamboo AGPL-3.0-only、alice-print proprietary) を `licenses.exceptions` で限定許可、bincode 1.3 unmaintained (RUSTSEC-2025-0141、`.cargo/audit.toml` と同期) を ignore
- `scripts/preflight.sh` 追加 (CI gate の local 逐語再現、pre-push hook が `--quick` で block)

### Changed
- **LOL grammar の copy 運用を廃止** `crates/llm/src/lol.gbnf` (手動 copy) を削除し、`grammar_lol::LOL_GBNF` は `alice_bamboo::LOL_GBNF` (= `alice_lol::LOL_GBNF`、feature 外 `include_str!`) の re-export に copy は ALICE-LOL 本体から 199 行 drift しており (comment / whitespace 厳格化、`program(...)` Intent wrapper なし)、逆に copy 側だけに product shortcut 65 個が足されて本体に upstream されていなかった (mechanical archetype 38 個はどちらにも無く、system prompt が案内するのに grammar ON だと emit 不能) → ALICE-LOL 側で 103 構文を canonical grammar に追加 + parser ⊆ grammar の drift guard test (`52e9834`) `enforce_lol_grammar` は default OFF のまま (iGPU 速度都合、[feedback_text_to_print_grammar_off_intentional]) `crates/llm` に `alice-bamboo` 依存追加
- **`system_prompt.md`**: 「NO `//` comments, NO indent: max 1 space between tokens」を追記 (canonical grammar が comment / 連続 whitespace を拒否するため)、example の indent 除去、reminder 2 行を短縮して 4488 chars (4500 予算内)

### Added
- **DfAM 測定 + 判定を 3MF export 経路に統合** (`alice_bamboo::dfam`、text-to-cad `dfam-check` 吸収) `MeshStats.dfam_summary` に壁厚 p05 / 最小穴径 / 最大ブリッジ span / サポート面積比 / 単位疑義 / watertight の findings (FDM 限界、ISO/ASTM 52910 準拠の保守値) Generate 画面に「DfAM (FDM): OK/NG …」行 + pass 以外の finding を表示
- **LLM retry loop に DfAM 違反を接続** `safety_check_lol` が Preview 解像度 mesh で DfAM を測り、`DfAM wall thickness / positive feature / hole diameter` の Fail を `fix_prompt` に流す (`SafetyViolationKind::{WallTooThin, FeatureTooSmall, HoleTooSmall, BridgeTooLong, NotWatertight}` 追加) bridge / watertight は retry trigger にしない (曲面形状で常時発火 / mesher の性質で LOL 設計の問題でないため)、UI + manifest 表示のみ
- manifest `safety_violations` に DfAM Fail メッセージを merge (LoRA 学習データの品質シグナル)
- **造形向き探索** (`alice_bamboo::dfam::evaluate_orientations`、軸整列 6 + 球面 32 候補、mesh を回さず造形軸を回す) 現在よりサポート面積が 20 %+ 減る向きがあれば `DfamSummary.orientation_hint` に `"rotate: -Z up → support 312 → 0 mm² (-100%), height 10.0 mm"` を格納し Generate 画面に表示

- **`ttp` headless CLI** (`crates/core/src/bin/ttp.rs`、clap 不使用) `ttp check --lol` (parse + safety + DfAM、JSON) / `ttp export --lol --out --format 3mf|stl|fbx|step|gcode --quality` / `ttp validate --gcode --bed h2d|h2d-dual|x1c|a1-mini` (alice_print 静的検証)、exit 0 / 1 / 3 (findings fail)
- **Agent Skill** `skills/text-to-print/SKILL.md` (+ `references/lol-dsl-quickref.md` = system_prompt mirror、`references/dfam-findings.md`) + `.claude-plugin/plugin.json` / `marketplace.json` — Claude Code / Codex から `ttp` 経由で LOL 作成 → DfAM check → export → validate を回せる (text-to-cad の配布形態を吸収、Free tier の入口)

### Changed
- `ROADMAP.md` P1-10 を実態に同期 (G-code export 配線済、ベッド配置変換 fix 反映)

## [v0.1.0-beta.4] - 2026-09-08

README docs polish + Zenn 過去記事から print result 実写画像を回収

### Added

- **README tagline 差替** (両 file 冒頭):
  - Ja: `自然言語から 3D プリント用ファイル (3MF / G-code) を直接生成する、完全ローカル稼働のデスクトップアプリ (小型 LLM 内蔵)`
  - En: `Fully-local desktop app that generates 3D print files (3MF / G-code) directly from natural language, with an embedded small LLM`
  - G-code / 完全ローカル稼働 を明示 (旧 tagline は Bambu 3MF only の記述)
- **`docs/images/print-result.jpg`** 追加 (273 KB、1330×997、JPEG)
  Zenn 過去記事 `alice-sdf-bambu-skadis-pegboard` (2026-07-27) から
  alice-bamboo pipeline で生成した SKADIS shelf 実使用中の写真を回収
  両 README で `Print result` section に landing (broken link 解消)

### Changed

- **English UI screenshot section** の image 参照を暫定 Ja UI 版 file
  (`hero.png` 等) に fallback (`-en` suffix 版は未撮影) note を
  「English UI screenshots pending capture、layout / controls は言語問わず
  同一」に更新 future release で置換予定
- **`Print result` caption** を「SKADIS panel 等、post-β」から実物内容
  (`SKADIS shelf、alice-bamboo pipeline で生成 → 印刷 → 実使用中`) に
  更新

### Fixed

- **Enterprise 問い合わせ先 email address 修正**: `enterprise@alicelaw.net`
  (実在せず、GAM `Service not applicable/Does not exist` 実測) →
  `contact@extoria.co.jp` (44 alias 集約先の canonical 法人窓口) 5 hit
  修正 (README.md / README.ja.md / ROADMAP.md / settings.rs const /
  settings.rs test) tag `v0.1.0-beta.4` は published 直後に revert +
  同 tag で打ち直し

### 4 gate

- test: 371 pass / 0 fail / 6 ignored
- clippy `--workspace --all-targets -- -D warnings`: 0 warnings
- fmt: clean
- release build: success
- wasm32 worker check: clean

## [v0.1.0-beta.3] - 2026-09-08

P1-11 完全 English i18n 対応 (Ja/En parity) の実装完了 全 UI 本体 630+
strings を Ja/En 対訳化、Runtime Lang switcher UI + env override + live
per-frame re-resolution を実装 β 期間の英語圏 user 拡大対応

### Added

- **完全 English UI i18n (Ja/En parity)**:
  - Phase 2 (`3cee74b`): `crates/app/src/ui/settings.rs` (1461 行) の
    hardcoded 日本語 115 strings を `T::settings_*` fn 経由に refactor
    (34 fn 追加、10 sub-fn signature に `lang: Lang` threading)
  - Phase 3 (`db6d5d1`): `crates/app/src/ui/prompt.rs` (4270 行) の
    hardcoded 日本語 566 strings を `T::prompt_*` fn 経由に refactor
    (428 fn 追加、94 fn signature に `lang: Lang` threading、61 archetype
    customizer 全て対応)
  - Phase 4 (`0430982`): `main.rs` 上部右の daily usage counter
    (`{:?} | {} 回`) i18n 化 (`header_usage_uncapped` fn)
  - Phase 5 (`0263c25`): Cloudflare Worker (`crates/worker/src/email.rs`)
    の Paid tier license 送信メール本文を English + 日本語 bilingual
    併記に変更 (locale detect 不要、── 区切り)

- **Runtime Lang switcher (Phase 6、`a2efbbd`)**:
  - `Settings > Language` collapsing section 追加 (Auto / 日本語 /
    English ComboBox)
  - DB `profiles.lang_pref` column (`auto` / `ja` / `en`) で永続化
  - env `APP_LANG=en` / `TEXT_TO_PRINT_LANG=en` override (OnceLock cache)
  - `App::update()` 毎に `resolve_lang(&state.lang_pref)` re-resolution =
    Settings 変更 live 反映 (再起動不要)
  - 優先順: env > DB > `sys_locale::get_locale`

- **README §Screenshots restructure (Phase 6 同時)**:
  - 「日本語 UI / Japanese UI」section + 「English UI」section の 2 grid
  - 「Print result (language-neutral)」独立 section
  - 英語 UI mode 起動手順明記 (Settings UI / env)

- **CAPTURE_GUIDE.md 12 shot rule (Phase 6 同時)**:
  - 6 shot × Ja/En 2 版撮影ルール
  - 英語 UI mode 起動手順 2 種 (Settings ComboBox / env variable)

### Changed

- **翻訳 nuance fix (`da60e29`)**: 箸ホルダー customizer En 側
  `pair` → `pairs` (pair_count range 2-10 常に複数、文法修正) 併せて
  label `pair count:` → `Pairs:` に統一

### Test coverage

- lang_pref DB persistence: 3 tests 追加 (default_to_auto / roundtrip /
  unknown_profile_returns_auto)
- workspace tests: 249 → 371 pass (Phase 2-6 で +122)

## [v0.1.0-beta.2] - 2026-09-07 (retroactive documentation)

以下 content は v0.1.0-beta.2 release (2026-09-07) 時点で既に landing
していたが CHANGELOG への転記が漏れていたため、beta.3 release 時に
retroactive 追記

### Added

**Multi-domain 4 domain 展開** (2026-08-27 〜 08-28、Sprint 21-22 continuation)

- **Mechanical domain** (Sprint 21 Phase X.1 + Sprint 22 Phase X.2、11 archetype + 18 primitive):
  - Layer A archetype: `vesa_mount` / `l_bracket` / `t_slot_bracket_2020` / `raspi_mount_plate` / `heat_set_array` / `flange_mount` / `dovetail_pair` / `profile_extrusion` / `snap_fit_pair` / `boss_array` / `bearing_seat` (ISO 4762 / 10642 / DIN 準拠、BearingKind 608/688/6001/6202)
  - Layer B primitive: `screw_hole` / `tap_hole` / `counterbore` / `countersink` / `heat_set_hole` / `bolt` / `bracket_l` / `flange_circular` / `t_slot_2020` / `profile_2020` / `profile_3030` / `dovetail` / `slot` / `snap_fit_annular` / `pin_hinge_knuckle` / `boss` / `rib` / `rack_shelf`
- **家具 domain** (Flat-pack Furniture、2 archetype + 1 primitive):
  - `cable_grommet(od, height)` (Ø60/Ø80 デスク配線グロメット)
  - `curtain_rod_bracket` (建築と兼用)
  - `dowel_hole(dia, depth)` (Ø8 標準 + 0.1mm glue expansion)
- **建築 domain** (Interior Mount、1 archetype + 1 primitive):
  - `curtain_rod_bracket(rod_dia, projection)` (Ø25/Ø30 カーテンレール壁掛け)
  - `wood_screw_pilot(screw_dia, hardwood)` (softwood 0.7× / hardwood 0.9× rule)
- **電子工作 domain** (Electronics Hobbyist、3 archetype + 1 primitive):
  - `arduino_mount_plate(board_type, extras)` (Uno/Mega/Nano 4-hole M3 + optional VESA)
  - `pixhawk_mount(size, damper_style)` (drone 45×45/30×30mm、Ø10 vibration damper)
  - `servo_mount(servo_type)` (SG90 mini / MG996R standard)
  - `jst_ph_slot(pins)` (JST-PH 2.0mm pitch、2/3/4/5 pin)

**MetricSize / BearingKind 拡張**

- `MetricSize::M2` (2.0mm、小型基板 / センサー)
- `MetricSize::M2_5` (2.5mm、Raspberry Pi / Arduino 標準)
- `BearingKind` enum (608ZZ / 688ZZ / 6001ZZ / 6202ZZ、ISO 15 / NSK / SKF spec)
- `from_f32_snap` helper for both (LLM 非規格値を最近接規格に snap)

**Preset library 大幅拡張**

- `default_presets.json`: **12 → 161 preset、3 → 25 category**
- 新 category: 機械要素 (VESA + Lブラケット) / 機械要素追加 (Sprint 21 + bearing) / 機械要素 building block (Phase X.2 demo) / 家具 (Flat-pack Furniture) / 建築 (Interior Mount) / 電子工作 (Electronics Hobbyist)

**Gallery Phase 1-3** (2026-08-26)

- Phase 1: nickname + `gallery_auto_share` preference (Settings > プロフィール)
- Phase 2: share confirm dialog (Free tier + share on + auto off で生成完了時 3-choice modal)
- Phase 3-Worker: Cloudflare Relay endpoint (`/api/gallery/{list,publish,:id}`) + ed25519 sig verify + 100KB LOL max + rate limit
- Phase 3-App: `gallery_client.rs` + gallery UI 書き換え + fork/delete + preview stub 解除

**開発者体験改善**

- Issue template 3 file (`bug_report.yml` / `feature_request.yml` / `config.yml`、日本語+英語併記、YAML form 形式)
- `scripts/smoke_worker.sh` (Cloudflare Worker 4 endpoint 疎通 test、shellcheck clean、TTY 判定 ANSI 色付け、GALLERY_DB 未 provision hint)
- `docs/GO_PUBLIC_CHECKLIST.md` (Phase 1-5 + Rollback、public 化当日手順集約、2026-08-24 新設、以降 Gallery / Issue template / GALLERY_DB provision 反映)

### Changed

- **`system_prompt.md`**: MECHANICAL / FASTENER / Low-level 3 section 追加 + Mech composition 1 行 example (`subtract(rounded_box(30,2.5,30,3), counterbore(4,5))`) + SHORTCUT list name-only 圧縮で iGPU 4500 byte budget 内 (4408 → 4496 bytes)
- **Preview renderer** (2026-08-25 系列): offscreen render + depth attachment で hollow shape 正確描画 + `renderer.write()` 再帰取得 deadlock (mouse spinner regression) 修正
- **README `#Status`**: β release 反映 + Sprint 12-20 batch (61 archetype 累積) + `#Build` に ALICE-Bamboo private repo 注意 + Releases DL 経路推奨
- **CONTRIBUTING.md**: 内部規約ラベル注記から特定業務語彙除去 (public 化 pre-flight)

### Fixed

- **CI Doc job**: rustdoc intra-doc link `[`presets_sync`]` (unresolved) → `[`spawn_presets_sync`]` (state.rs:3044、`RUSTDOCFLAGS=-Dwarnings` で hard error 化していた CI red 修正)
- **CI Fmt job**: main.rs / sdf/renderer.rs の cargo fmt 未適用差分 (CI red 解消)
- **UI Preview**: renderer.write() 再帰取得 deadlock によるマウス spinner 化 regression 修正

### Infra

- **GALLERY_DB provision** (2026-08-28): `wrangler d1 create text-to-print-gallery` (APAC region、`e6d81f9a-717f-4c2a-a544-37cecf73c260`) + migration 実行 + `wrangler deploy` + `smoke_worker.sh` 4/4 pass verify
- **PRESETS_KV re-seed** (2026-08-28): production Worker が 25 category / 161 preset 返却 (2026-08-23 seed の 3 category / 12 preset から更新)
- **Test infrastructure**: workspace scope 4 gate 規律徹底 (`cargo test --workspace --lib` 必須、押韻 4 単語 → 8 単語に格上げ)、`presets_client::parse_bundled_default_matches_worker_schema` の count assertion を JSON asset 変更と同 commit 内で追随
- **Cross-repo**: ALICE-LOL 6 commit (edc29cc / 373fd91 / b111a62 / c9707df / 6c8a7ff / 75a51a4) 累積 test 470 → 601 (main lib 572 + humanoid 29)

## [0.1.0] - 2026-08-22

初回 β release 7 artifact 公開 ([GitHub Releases v0.1.0](https://github.com/ext-sakamoro/text-to-print/releases/tag/v0.1.0))

### Artifacts

- macOS Apple Silicon `.tar.gz`
- macOS Intel `.tar.gz`
- Windows `.msi`
- Windows `.zip`
- Linux `.tar.gz`
- Linux `.deb`
- Linux `.AppImage`

合計 137 MB

### Highlights

- Standalone desktop app (SaaS backend 不要、Cargo で単体 build)
- Embedded ALICE-LLM (Qwen 3.5-4B Q4_K_M) で自然言語 → LOL DSL → Bambu 3MF 生成
- BYO LLM 3 経路 (Sidecar / Embedded / API 持参 Claude・OpenAI・Google・Ollama・Custom GGUF)
- Sprint 1-20 で 61 archetype (生活雑貨 / 趣味 DIY / 工具 / 電子機器 / バスルーム / キッチン / 3D プリンタ周辺 / 引き出し・壁 / ミックス 9 種) 実装済
- MakerWorld 対応 12-file zip 3MF 直接生成 (`alice_bamboo::bambu_3mf::export_bambu_3mf`)
- macOS/Windows 無署名 (Apple Developer Program / Authenticode 加入は v1.0 予定)

### Notes

- Publish job は GitHub Actions billing 制限で fail、local `gh release create` 経由で publish
- 7-patch iteration (`d0a2787` → `a70a7ea`) で cross-platform build 完走 (rust-toolchain pin + msi version strip + macOS 署名 optional 化 + main.wxs revert + cargo wix `-p` 復活)

## Links

- Repository: <https://github.com/ext-sakamoro/text-to-print>
- Releases: <https://github.com/ext-sakamoro/text-to-print/releases>
- Support: <https://ko-fi.com/sakamoro>
