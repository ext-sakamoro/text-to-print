# Changelog

本 file は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) 形式に準拠し、
バージョン管理は [Semantic Versioning](https://semver.org/lang/ja/) に従う

## [Unreleased]

### Fixed — licence を支払いが済んだ checkout にだけ、1 回だけ発行する (2026-10-25)

- `checkout.session.completed` は `payment_status` が `paid` か `no_payment_required` の時だけ licence を発行する (`unpaid` の非同期決済は `checkout.session.async_payment_succeeded` で発行、`async_payment_failed` は記録だけ) これまでは決済の状態を見ずに発行していた
- webhook の処理が失敗した event は処理済みに記録せず 500 を返す (Stripe が再送する) これまでは失敗も記録して 200 を返していたので、一時的な失敗で key が届かなかった
- licence は発行の鍵 (`checkout:<session id>` / `subscription:<id>:<期間の終わり>`) ごとに 1 行 (migration `0005_license_issue_key.sql`)、再送では保存済みの key を送り直す licence の mail の送信の失敗は誤りとして返す

### Fixed — 有料の tier を起動のたびに licence key で決める (2026-10-25)

- 起動時の tier は保存した licence key を署名と有効期限で検証し直して決める (これまでは貼り付けた時に 1 回だけ検証し、その後は profile の `tier` 列をそのまま使っていたので、期限を過ぎても有料の tier のままだった) 無効・期限切れの key は Free
- 署名は受け取った payload の byte 列そのものに対する厳密な Ed25519 (alice-sign の `verify_strict`) で検証する (payload を直列化し直した byte 列では検証しない) 返す payload も署名された byte 列から読む 発行済みの key はそのまま通る
- `LicenseVerifier::verify_at(key, now)` と `license::tier_of_stored_key` と `Database::load_tier` を足した `verify` は payload を値で返す

### Fixed — gallery の並び順を worker が受け取った時刻で決める (2026-10-25)

- 一覧は `received_at` (worker が付ける時刻) の新しい順 (著者が署名した `created_at` で並べていたので、未来の日付の投稿が先頭に残り続けた) migration `0004_gallery_received_at.sql` が索引を足す
- `created_at` は UTC (`Z` か `+00:00`) だけを受け、worker の時計より 10 分を超えて先の時刻は `gallery_created_at_future` で拒む 他の offset は `gallery_created_at_format` desktop は `Z` 付きの UTC (ミリ秒まで) を送る
- 投稿の hourly 上限は著者 (did、`GALLERY_PER_AUTHOR_HOURLY`) と client の address で数える (client が選ぶ post id では数えない) 削除も address の上限に数える

### Added — 公開の文の検査を CI と preflight で実行する (2026-10-25)

- `.github/workflows/public-text.yml` と `scripts/public_text.sh` が、ALICE-Law の `scripts/public_text_check.py` を `.github/public-text-pin` の rev と SHA-256 で固定して取り、HEAD の全 file と前回からの commit を検査する (比較 0 件・pin の形の誤りも失敗) `scripts/test_public_text.py` がその script の試験 3 file は ALICE-LOL ほかと同じ中身
- preflight に同じ検査と試験を足した

### Changed — 非公開の sibling を deploy key で取る (2026-10-25)

- CI / release の 6 本の workflow は、非公開の ALICE-Bamboo / ALICE-Print を repo ごとの read-only の deploy key (secret `ALICE_DEPKEY_BAMBOO` / `ALICE_DEPKEY_PRINT`) で checkout する 鍵はその checkout の step だけが受け、`persist-credentials: false` で step の後に残さない 公開の sibling の checkout は資格情報を受けない
- `.github/siblings.toml` の `[private]` に非公開の sibling と鍵の input を載せ、`scripts/ci_sibling_checkout_check.py` が composite action と呼び出し側の両方を照合する (公開の checkout が資格情報を受ける / 非公開の checkout が鍵以外を受ける / `persist-credentials: false` が無い / 呼び出し側が別の secret を渡す、のどれでも失敗)
- `CONTRIBUTING.md` と `.github/ISSUE_TEMPLATE/` を外した Releases への link は `/releases` を指す

### Changed — 画像を sRGB にする (2026-10-25)

- `docs/images/` の PNG 6 枚の色を、埋め込まれていた表示の profile から sRGB に変換した (relative colorimetric、alpha はそのまま) 画素の値が変わる file は IHDR / sRGB / IDAT / IEND だけにした JPEG 1 枚は profile が sRGB なので segment を外しただけで画素は変わらない
- `scripts/image_metadata_check.py` (試験 `scripts/test_image_metadata_check.py`): 追跡している PNG / JPEG が許した chunk / segment 以外を持てば失敗 (検査した画像が 0 件も失敗) CI の wiring-guard job と preflight で実行する

### Security — LLM model の download を commit と SHA-256 で固定する (2026-10-25)

- これまで model は Hugging Face の `resolve/main/{file}` (動く branch、対象の多くは第三者の repo) から取り、中身を照合せずに置いていた 差し替えられた GGUF がそのまま llama の parser に渡り、途中で切れた応答も完成品として残って以後は取り直さなかった
- `ModelChoice::pinned_file()` が choice ごとに Hugging Face の commit・SHA-256・size を持つ (2026-10-25 に Hugging Face が示す値を記録) download は `resolve/{commit}/{file}` から一時 file に取りながら SHA-256 を計算し (ALICE-Crypto の `sha256::Hasher`)、size と SHA-256 が一致した時だけ model にする 一致しなければ一時 file を消して失敗する Content-Length が違う応答は書く前に、長さの無い応答は固定した size を超えた時点で止める
- 既にある file も照合する 照合の結果は file の横に印 (SHA-256・size・更新時刻) として残し、`model_exists` は印が今の file と一致する時だけ真を返す 一致しない file は消して取り直す 手で置く choice (Bonsai 27B) は照合の基準が無いので、file があれば使う (download 元も無い)
- GGUF を開く経路 (Embedded の読み込みと sidecar の起動) は `ModelFile` だけを受け取る `ModelFile` を作れるのは `download_model` / `verified_model` (固定のある choice は照合が通った時だけ) と、利用者が path で選んだ file の `ModelFile::user_supplied` だけ `model_path` は crate の外に出さない 実行中に切り替えた choice も照合を通らなければ読み込まない (以前は切り替えた choice の file を照合せずに読み込んでいた)
- 試験: 一致する応答は残る / 1 byte 違う・短い・長い・Content-Length より短い応答は失敗して何も残らない / 長さの無い長い応答は固定の size で止まる / 照合前の file と照合後に書き換えた file は model にならない / 違う bytes の既存 file は取り直して置き換え、取り直しが失敗したら model の path に何も残さない / 全 choice の commit・SHA-256・size の形と URL が branch を使わないこと sibling に ALICE-Crypto を足し、`deny.toml` に licence の例外を足した

### Security — Gallery の署名が投稿の全項目を覆う (2026-10-25)

- Gallery の投稿の署名は `id|did|lol_source|created_at` を `|` で連結した文に掛けていた `author_nickname` は署名に含まれず、途中で名前を差し替え・削除しても受け付けていた また `lol_source` に `|` を含めると、署名を保ったまま `lol_source` と `created_at` の境目を動かせた
- 署名する文を `crates/gallery-sign` に一本化し、desktop client と Worker の両方がそこから作る 文は ALICE-Serial の canonical encoding で、domain の文字列と全項目 (`id` / `author_did` / `author_nickname` / `lol_source` / `created_at`) を順に並べた配列にする nickname が無い時は `Null` 項目ごとに長さが付くので境目は動かせない 削除は別の domain なので、投稿の署名は削除の署名にならない 署名の bytes は試験で固定する
- 検証は ALICE-Sign の `verify_strict` (正規でない scalar と small-order の鍵を拒否する)
- `created_at` は RFC 3339 の形と各項目の範囲 (月ごとの日数と閏年を含む) を確かめ、それ以外を拒否する
- **この変更より前の版の client が作った署名は受け付けない** (署名する文が変わった)
- Worker の単体試験を CI の check-wasm-worker job と preflight で host で実行する (署名の範囲の試験 2 本が `ok` で出ることを確かめる) sibling に ALICE-Serial / ALICE-Sign を足し、`deny.toml` に licence の例外を足した

### Changed — Enterprise の問い合わせ先と作者の表示、ROADMAP.md を外す (2026-10-24)

- licence の file を `LICENSE` から `LICENSE-Proprietary` に改名した (README / README.ja / CONTRIBUTING と deb の同梱の参照も直した)
- 設定画面から料金と提供予定の文言を外した (`settings_pro_planned` / `settings_pro_coming_soon` / `settings_coming_soon` を削除、購入 button の表示から金額を外し、無効時の hover は「現在は購入できません」、crash report の説明から送信予定の記述を外した) 試験 `no_ui_text_states_a_price_or_a_plan` が i18n の全 string literal (1,500 件余り) に金額・版・予定の語が無いことを確かめる
- `docs/DESIGN.md` を repository から外した (README / README.ja の tree の行も直した) README / README.ja の料金の節は表だけにした
- `ROADMAP.md` を repository から外した (README / README.ja の tree と参照、`docs/RELEASE.md` の残タスクの節、`docs/STRIPE_SETUP.md` の関連の行も外した) 作者の表示から e-mail を外した (`crates/app/Cargo.toml` の authors と deb の maintainer、設定画面の Author の行は "Moroya Sakamoto" のみ)
- Enterprise の問い合わせ先を e-mail でなく web page (https://extoria.co.jp/) にした (README / README.ja の表、設定画面の「Contact for Enterprise」の行き先 `ENTERPRISE_CONTACT_URL`、旧 `ENTERPRISE_MAILTO`) `crates/worker/wrangler.toml` の account の注記から e-mail を外した

### Changed — 新しい ci/ branch の最初の送信でも fuzz / security-audit を走らせる (2026-10-24)

- `fuzz.yml` / `security-audit.yml` の on.push から paths filter を外し、`scripts/changed_paths.py` (ALICE-Crypto と同一) と `.github/paths/<workflow>.txt` で実行可否を決める changes job と gate job に移した 新しい `ci/**` branch の最初の送信 (既に別の branch にある commit を含む) では GitHub の filter が空の差分を見て run が作られていなかった CI (actionlint job) と preflight で `scripts/test_changed_paths.py` と `scripts/changed_paths.py sync` を実行する
- fuzz の job から continue-on-error を外した crash は失敗にする (見つかった欠陥で、後で読む報告ではない) security-audit の coverage は参考の検査のまま、continue-on-error を job から step に移し失敗を warning の注釈で出す (pipefail を付けた)
- semver-checks (公開 API の互換性) を security-audit.yml の job から独立した `semver-checks.yml` に移し、手動起動 (workflow_dispatch) だけにした push / pull request / 週次では走らせず、changes job と gate も持たない 参考の検査のまま、失敗は step の warning の注釈で出す (常に成功にしていた `|| true` は外した)

### Added — deny.toml の licence の例外を依存の宣言と照合する (2026-10-24)

- `scripts/deny_exception_guard.py` (試験 `scripts/test_deny_exception_guard.py`): `[[licenses.exceptions]]` の各例外が許す id は、その crate が `cargo metadata` で宣言する license の式に含まれる id だけにする (宣言に無い id、依存に居ない crate や global の allow が既に受ける crate の例外、例外の要る alice-* の crate に例外が無いことはどれも失敗、比較 0 件も失敗) sibling の固定 commit を上げて依存の license が変わった時は、どの例外を同じ commit で変えるかを名指す CI の deny job と preflight で cargo deny の前に実行する 今の例外はすべて宣言と一致する

### Changed — ライセンスを Proprietary にする (2026-10-23)

- text-to-print のライセンスを MIT から `LicenseRef-Proprietary` (All rights reserved、ライセンスの許諾なし) にした `LICENSE` を差し替え、workspace と `crates/worker` の `license` を変え、全 crate を `publish = false` にした README / README.ja / CONTRIBUTING と app の About 画面の表記も合わせた
- **この変更より前に公開した版 (0.1.0-beta.4.1 まで) は公開時のライセンス (MIT) のまま** 履歴は書き換えない
- `deny.toml`: 公開しない workspace の crate を license 検査から外す (`[licenses.private] ignore = true`) sibling の例外 4 件は変えない

### Security — LAN への自動配信を取り除く (2026-10-23)

- これまでの版 (v0.1.0-beta.4.1 までの全リリース) は、General tier で生成するたびに prompt (入力文) と LOL DSL と DID を、同じ LAN で動いている text-to-print に送っていた (mDNS で見つけた相手へ libp2p の gossipsub で配信、利用者の確認なし) Free / Pro / Enterprise tier は送っていない
- どの tier でも、起動すると全 interface (0.0.0.0) の TCP で待ち受け、LAN の相手から届いた LOL DSL を data directory の `cache/` に書いていた
- この peer-to-peer 層をすべて取り除いた (libp2p と、`node` / `sync` / `vcs` / `cdn` / `cache` の module、tier の `force_public`) 生成で何かを送るのは利用者の同意がある時だけで、送り先は HTTPS の学習用の収集先 (`/api/share`) と Gallery (`/api/gallery/publish`) 既存の data directory に残る `cache/` と `dag.json` は読まれない (消してよい)
- 同梱の LLM sidecar (`alice-llm-server`) も全 interface (`*:8000`) で待ち受けており、app を signal で止めると sidecar が残って待ち受け続けていた sidecar を `--host 127.0.0.1` で起動して loopback だけで待ち受けるようにし、空き port の確認も 127.0.0.1 で行う sidecar の stdin を piped にして書き込み側を app が持ち、`--exit-on-stdin-eof` で起動するので、app が正常終了・SIGTERM・SIGKILL・異常終了のどれで止まっても OS が pipe を閉じて sidecar も止まる app が待ち受けるのはこの loopback の sidecar だけ (model が無い時は sidecar も起動しない) `crates/llm/tests/sidecar_lifecycle.rs` は実 binary で、app 役の process を SIGTERM / SIGKILL で止めて sidecar の process と port が消えることを確かめる (CI は sidecar を build して別 step で走らせる) ALICE-LLM 側の変更 (既定の待ち受けを 127.0.0.1、`--host`、`--exit-on-stdin-eof`) を含む commit に sibling の固定を移した
- `scripts/no_p2p_guard.py` が Cargo.lock の package と追跡している `Cargo.toml` / `.rs` を見て、libp2p が戻ると CI (ci.yml) と preflight を失敗させる (検査 0 件も失敗)
- libp2p だけが引いていた advisory の ignore を `deny.toml` / `.cargo/audit.toml` から外し、RUSTSEC-2026-0382 (rustls-webpki 0.101.7) の保留を消した

### Fixed — yank された clipboard-win を外す (2026-10-23)

- Cargo.lock の tokio 1.53.1 を 1.53.3 にした (RUSTSEC-2026-0386、`AsyncFd` の構築子が I/O safety を破る unsound、修正版は >=1.53.3) lock の差は tokio の 1 項目だけ
- Cargo.lock の clipboard-win 5.4.1 (crates.io で yank、eframe 0.31 → egui-winit → arboard 経由、Windows だけ) を 5.4.3 にした lock の更新だけ cargo deny の `yanked = "deny"` で Security & Hygiene が失敗していた

### Changed — rfd を 0.17 にする (2026-10-23)

- file dialog の rfd を 0.15.4 から 0.17.2 にした 0.17 は Linux の xdg-desktop-portal を libdbus で直接呼ぶので、zbus 5.18 / ashpd 0.11 と zvariant 5.13 / zbus_names 4.3 / zvariant_derive 5.13 が lock から消えた feature は `xdg-portal` だけにした (`wayland` は親 window の指定にしか使わず、app は指定していない) 呼び出し (`FileDialog::new().add_filter().pick_file()` / `set_file_name().save_file()`) は変えていない
- RUSTSEC-2026-0370 / 0371 / 0372 の保留を zbus 4.4 の経路 (eframe 0.31 → egui-winit → accesskit_unix → atspi、Linux の読み上げ) の 3 件だけに絞った 解消は eframe / egui の版上げ

### Changed — 共有の同意を型で求める (2026-10-23)

- queue に入れる `enqueue` と同意の型 `ShareConsent` を app の確認 dialog の module (`ui::share_confirm`) に置き、`enqueue` は `ShareConsent` を受け取る 同意は確認 dialog で公開を押した時 (`pressed_publish`、この module の外からは呼べない) か、利用者が自分で自動送信を on にした時 (`ShareConsent::standing`、既定 off) にだけ作れる network の crate は queue を読んで送るだけで、書き込む公開の関数を持たない 自動送信は Settings の切り替えと dialog の「以降は確認なしで送信」でだけ on になり、tier の変更や DB の migration では変わらない
- Settings で自動送信か LoRA share を off にすると、まだ送っていない queue の中身を送らずに捨てる off の処理は `AppState::turn_off_share` の 1 経路だけで、設定を保存してから queue を捨てる (設定を書けなくても queue は捨てる)
- 確認 dialog・自動送信・LoRA share の説明に、送る項目をすべて書いた 項目は送る request の struct (`SharePayload` / Gallery の publish) を JSON にした時の key から作る (`share::sent_field_names` / `gallery_client::publish_field_names`) ので、field を足すと説明にも出る (`uuid`、`quality.success`、`quality.time_to_file_ms`、`quality.export_format`、`quality.user_kept`、`quality.user_edited` などが以前の説明に無かった) 公開を押すと学習用の収集先 (`/api/share`) と Gallery (`/api/gallery/publish`) の 2 か所に送ることも書いた 自動送信で送るのは学習用の収集先だけで、Gallery には載らない
- 「Paid tier は完全 offline」の説明を消した (README / Settings / docs) どの tier でも起動時に preset の同期・Gallery の一覧・更新の確認の通信がある 送らないのは生成のデータ

### Added — 3MF の日付を入力にする (2026-10-23)

- `pipeline::export_mesh_dated(lol, dir, format, quality, date)` と `pipeline::ExportDate` を足した 3MF の `3D/3dmodel.model` に入る `CreationDate` / `ModificationDate` を呼び出し側が渡せる (ALICE-Bamboo の `export_bambu_3mf_dated`) `export_mesh` は今日の日付 (UTC) を渡すだけで、出力は変わらない
- `tests/zip_oracle.rs` は記録した日の日付 (2026-10-16) を渡して書き出し、日付を含む `3D/3dmodel.model` も書かれたまま比べる 記録済みの oracle は変更なしで一致する 今までは今日の日付が入るため、記録の翌日から一致しなかった 伏せるのは実行ごとに変わる object 名 (UUID v7) だけ

### Changed — ALICE-Physics 4.0 の package の分割に追従し、sibling の組を上げる (2026-10-23)

- `.github/siblings.toml` の組を ALICE-Bamboo c7d45a9 / ALICE-LLM 01b8c7d / ALICE-LOL bf2aeb5 / ALICE-Physics d3f56c0 / ALICE-SDF d573ac4 / ALICE-Zip a59ba9c にした (ALICE-Print 3e5f9c0 は据え置き) Cargo.lock と fuzz/Cargo.lock の解決は alice-llm 2.0.1 → 3.0.0 / alice-lol 0.4.2 → 0.5.0 / alice-lol-macro 0.3 → 0.4 / alice-physics 3.1 → 4.0 (+ alice-physics-core 4.0) / alice-sdf 6.1 → 7.0 / alice-zip 0.10 → 0.11 text-to-print 側の source の変更は無い
- ALICE-Physics 4.0 で repository の root の package が `alice-physics-core` になり、窓口の `alice-physics` が `crates/alice-physics` に移ったので、`alice-physics` の path 依存を `../ALICE-Physics/crates/alice-physics` にした `deny.toml` の license の例外は宣言どおり `AGPL-3.0-or-later` だけを許すよう絞り、`alice-physics-core` に同じ例外を足した (許す範囲は広げていない)
- 3MF の書き出しは版上げの前後で全 entry の中身が一致する (`tests/zip_oracle.rs` の 3 program × 16 entry、mesh を含む) 違いは `3D/3dmodel.model` の日付 (`CreationDate` / `ModificationDate`) だけで、これは書き出した日の日付による
### Security — 新しい RustSec の advisory への対応 (2026-10-23)

- Cargo.lock の rustls-webpki 0.103.15 を 0.103.16 に、webpki-roots 1.0.9 を 1.0.10 にした (RUSTSEC-2026-0382 / RUSTSEC-2026-0381 の修正版、lock の更新だけ)
- 依存の版上げなしでは直らない 4 件を期限付きで保留した: RUSTSEC-2026-0382 の rustls-webpki 0.101.7 (libp2p 0.54 の libp2p-tls 0.5 だけが使う、0.101 系に修正版なし) と、保守が終わった zvariant / zbus_names / zvariant_derive (RUSTSEC-2026-0370 / 0371 / 0372、eframe 0.31 と rfd 0.15 が使う zbus 4.4 / 5.18 経由) 保留は `scripts/advisory_holds.toml` に advisory・crate の版・その版の依存元・期限 (2026-12-31) を書き、`scripts/advisory_hold_guard.py` が Security & Hygiene の cargo deny の前で照合する 保留した版が lock から消えた / 依存元が増えた・減った / 同じ crate の別の版が該当する / 期限を過ぎた / `deny.toml` と `.cargo/audit.toml` の ignore と保留が食い違う、のどれでも失敗し、保留が 0 件や lock が無い時も失敗する 別の版の判定は advisory が公開する修正版の範囲 (pre-release を含む semver の順序) で行い、依存元は直接の依存元だけを比べる cargo deny は `--locked` で、guard が読んだ lock をそのまま使う 保留はこの id と版だけで、他の advisory は従来どおり cargo deny / cargo audit の step を失敗させる 解消は libp2p 0.57 への版上げ (crates/network) と eframe / egui・rfd の版上げ (crates/app)

### Changed — ALICE-Bamboo / ALICE-Physics の pin を上げる (2026-10-17)

- ALICE-Bamboo を 48b18b5 にした 3MF に埋める template (`scripts/bambu_h2d_template_project_settings.json`) が Windows の checkout で CRLF になり、Windows で build した 3MF の `Metadata/project_settings.config` だけ行数分 (1508 byte) 大きかった ALICE-Bamboo 側で `.gitattributes` (追跡する全 file を `eol=lf`、画像と 3MF は binary) と試験を足した版 (`tests/zip_oracle.rs` の windows-latest で検出)
- ALICE-Physics を 4503f4f (浮動小数点の環境の検査を足した版、3.1.0 のまま) にした Cargo.lock は変わらない

### Changed — zip を 9 にする (2026-10-17)

- workspace の `zip` を 2.x から 9 にした (2.x の版はすべて crates.io で yank されている) ALICE-Print 3e5f9c0 と ALICE-Bamboo 6baf3d5 も zip 9 で、各 entry の更新時刻を固定する (`.github/siblings.toml` の pin) Cargo.lock の zip は 9.0.4 の 1 版だけになった entry の名前を返す `name()` が `Result` を返すようになったので `manifest.rs` を合わせた
- `manifest::embed_in_3mf` は書き直す archive の各 entry の更新時刻を 1980-01-01 00:00:00 に固定する 今までは zip の既定に任せていて、zip の `time` feature が有効な build では書き出した時刻が入り、同じ program から別の archive ができた 試験の build は `crates/core` の dev-dependency で `time` を有効にするので、時刻を固定しなければ `tests/zip_oracle.rs` が落ちる (text-to-print 側と ALICE-Bamboo 側の固定をそれぞれ外すと、どちらも落ちることを確かめた) 通常の build の zip の feature は変わらない

### Changed — ALICE-LOL 0.4.2 / ALICE-Physics 3.1.0 / ALICE-SDF 6.1.0 / ALICE-Zip 0.10.0 / ALICE-Bamboo に追従する (2026-10-17)

- `.github/siblings.toml` の pin と Cargo.lock を ALICE-LOL 0.4.2 / ALICE-Physics 3.1.0 / ALICE-SDF 6.1.0 / ALICE-Zip 0.10.0 / ALICE-Bamboo 9ff48ee にした ALICE-Bamboo 9ff48ee がこの 4 つの版を要求するので 1 度に上げる (どれか 1 つだけでは依存が解決しない) text-to-print 側の code の変更は無い
- `crates/core/tests/empty_mesh.rs`: 形を包む箱が求まらない形の fixture を `polar_repeat(6, translate(8, 0, 0, sphere(1.5)))` から `gyroid(3, 1)` に替えた ALICE-SDF 6.1.0 が `PolarRepeat` に解析的な外接箱を与えるようになり、その形は書き出せるようになった (それを確かめる試験 `polar_repeat_is_bounded_and_exports` を足した) 替わりの形は、ALICE-SDF 6.1.0 で解析的な外接箱を持たない演算を source から列挙し、LOL の 226 構文の最小の program を Preview で書き出して選んだ 箱の張り付きを判定しない変異で試験が落ちることを確かめた

### Changed — ALICE-LLM を 2.0.1 にする (2026-10-17)

- 依存の ALICE-LLM を 1.6.0 から 2.0.1 にした (`.github/siblings.toml` の pin と Cargo.lock) text-to-print 側の code の変更は無い (workspace 全体の build と `text-to-print-llm` の試験 100 件、`tests/zip_oracle.rs` は通る)

### Added — 3MF の archive の中身を記録した oracle と比べる試験 (2026-10-17)

- `crates/core/tests/zip_oracle.rs`: 3 つの program (`crates/core/tests/fixtures/zip_oracle/*.lol`) を 3MF に書き出し、archive の entry ごとの名前・順序・圧縮方式・更新時刻・展開後の大きさ・中身の SHA-256 を記録 (`zip2.json`、zip 2.x の時点の書き出し) と比べる zip の library や書き出しを変えても中身が変わらないことを確かめるためのもので、圧縮後の byte は比べない (deflate の実装が違えば同じ中身でも byte が変わりうる) 書き出しは実行ごとに新しい UUID v7 を object の名前にするので、比べる前にその名前を伏せる 伏せるのは 1 つの archive に 1 種類の UUID が、記録した 5 か所 (model の Title、`Metadata/model_settings.config` の `name` 2 か所と `source_file`、`Metadata/plate_1.json` の `name`) にある時だけで、それ以外の場所や 2 種類目があれば失敗する app の書き出し (`export_mesh_with_metadata`、`manifest::embed_in_3mf` で archive を書き直し manifest を足す) も同じく `zip2_app.json` と比べる (17 entry、すべて無圧縮) 実行ごとの値 (作成時刻・所要時間・archive の hash・object 名からの id) を持つ `3D/3dmodel.model` と `Metadata/alice_manifest.json` は名前・順序・方式・時刻と構造だけを見て、他の entry は中身を比べる (object 名は 4 か所) 試験の歯: entry の順序・中身・圧縮方式・伏せる場所の数を記録で 1 つずつ変える、UUID の判定を緩める、のそれぞれで失敗する

### Security — rustls を 0.23.46 にする (2026-10-15)

- 依存の `rustls` を 0.23.45 から 0.23.46 にした (`Cargo.lock` のみ、要求は変えていない) RUSTSEC-2026-0346 / RUSTSEC-2026-0347 / RUSTSEC-2026-0348 / RUSTSEC-2026-0349 が 0.23.45 を対象にしており、cargo-deny の advisories の検査が失敗していた

### Changed — CI の job に上限時間を付ける (2026-10-14)

- CI: 全 workflow の 14 job に `timeout-minutes` を付けた (それぞれ最近の所要時間の 3〜5 倍、最低 10 分) これまで 1 job を除き上限が無く、止まった step が runner を GitHub の上限の 6 時間まで占有しえた `scripts/workflow_timeouts.py` が全 job の上限を検査し (job を 1 つも読めなければ失敗、検査器の試験 7 本)、CI の wiring-guard job と `scripts/preflight.sh` で走る
- `scripts/test_wiring_guard.py`: 所要時間を CPU 時間で測る試験は、検査が戻った後にしか上限を読めないので、戻らない検査 (待ち続ける・回り続ける) を捕まえられなかった 検査を thread で走らせ、壁時計で 600 秒を超えたら失敗する上限 (`within_wall_backstop`) を足した 待ち続ける検査で失敗することを試験する

### Changed — 空の形を誤りにする (2026-10-12)

- **Behavior change:** 読めた LOL から作った mesh が空の時、空の mesh を返さずに `pipeline::EmptyMesh` の誤りにする (preview /
  3MF の書き出し / 解析なしの書き出し)  形を包む箱を区間評価で絞れず探索の上限 (±500 mm) に張り付いた場合 (例: `polar_repeat`) と、
  材料が無い場合 (引き算で全部消えた等) を分け、利用者向けの文 (ja / en) は `violation_text.json` の errors から

### Added — 解析なしの 3MF の書き出し (2026-10-12)

- `pipeline::export_3mf_without_analysis`: 安全性 / overhang / DfAM / 向き探索 / 印刷可能性の解析をせずに 3MF だけを書く
  `export_mesh` の 3MF と同じ mesh を同じ書き方で書く (同じ file 名なら同じ bytes を試験)  解析は書き出しの所要時間の
  大半を占める (同梱 template で 3MF の書き出しの 5〜9 倍)  `ttp export --skip-analysis` (3MF のみ)
- `export_3mf_without_analysis` の file 名は LOL と品質の SHA-256 から決める (`model-<16 桁>.3mf`)  3MF は file 名を model 名として
  中に書くので、同じ LOL と品質からは同じ名前・同じ bytes の 3MF になる

### Added — 形を作る処理の進み具合と中止 (2026-10-12)

- `pipeline::preview_lol_to_mesh_with_progress` と `progress` module (`Phase` / `Progress` / `Weighted` / `Cancelled`)
  段 (読む / 箱 / 固体の探索 / mesh / 修復) と、段の中で実際に済んだ作業量 (固体の探索の区間評価の回数、成分ごとの mesh、
  再メッシュの回数) を報告する  数えられない処理 (1 回の meshing / 修復の中) は段に入ったことだけを報告し、割合は進めない
  全体の割合は段ごとの重み (同梱 template の実測の中央値) から作り、減らず、終わり (1.0) は 1 回だけ
- 報告の受け手が中止を返すと `Cancelled` で終わり、途中の mesh は返さない  中止が効くのは次の報告の時点
  (同梱 template では数えられない処理の最長が host で 3.2 秒、中央値 0.1 秒)
- 報告の有無で mesh は変わらない (bytes で一致を試験)  `preview_lol_to_mesh` は報告なしで同じ処理を呼ぶ
- `ttp preview --lol <file|-> [--quality …] [--progress]`: 表示用の mesh の頂点数・三角形数・digest (位置の bits と index の
  SHA-256) を JSON で出す  `--progress` は進み具合を 1 行 1 JSON で stderr に出す
### Changed — 検査の結果を構造で返し、利用者には平易な文と対処を出す (2026-10-12)

- **Behavior change:** desktop の生成後の統計欄は、検査の結果を「印刷の前に確認してください」の下に平易な文 (ja / en) と
  対処で出す  重大度で印を分ける (⚠ 直す / △ 確認 / ⓘ 参考)  検査の元の出力 (DfAM / Printability の要約と文字列) は
  「詳細 (検査の結果)」の折り畳みの中だけに出す
- `pipeline::safety_report` を追加: `safety_check_lol` と同じ検査の結果を `violation::Violation` (種類 / 重大度 / 確かさ /
  測定値 / 測り直した値 / 限界 / 位置 / 件数 / 元の文字列) で返す  `safety_check_lol` の文字列は従来と同じ bytes
  (同梱 template 116 件で一致を確認、LLM の修正の指示はこの文字列で分類する)  `MeshStats::violations` も追加 (3MF export)
- 印刷可能性の判定の方針を `crates/core/data/printability_policy.json` の 1 つの表にした  限界 (肉厚 1.6 / 穴径 2.0 /
  突起 0.8 mm、FDM の design guides の保守的な既定値) と出所、重大度の決め方 (限界から 10% 以内は advisory、
  反り High は advisory など、製品で選んだ値は `ours` の印) を持ち、DfAM と肉厚の検査もこの表から限界を受け取る
- 穴径 / 突起の標本は、報告された位置を含む最も大きい内接球で測り直す  標本が入隅 (床と壁の両方に近い点) を読んで実際より
  狭く出た時に本当の幅を出し、限界を満たすなら「測り方による誤差の可能性」として参考に下げる
- 利用者向けの文は `crates/core/data/violation_text.json` の 1 つの表 (desktop と iOS で共用)  同梱 template の意図した形の
  注記の表 `crates/core/data/template_annotations.json` を追加 (現在は空)

### Changed — 外部連携の名前を許可リストの箇所に限る (2026-10-12)

- `scripts/vocab_lint.py` を追加 (ci.yml の wiring-guard job と preflight)  追跡 file の本文と file 名を走査し、外部連携の名前を
  含む token が許可リスト (`<path>` と token の組の SHA-256 と出現回数) の外にあれば fail する  走査 0 件も fail
  許可するのは製品が必要とする箇所 (BYO LLM の provider 名・model id と画面の文言、agent 連携の plugin / skill、その試験) のみ
- provider について書いた code comment は名前を出さない書き方にした

### Changed — 配線ガードは "." で始まる directory を走査しない (2026-10-12)

- `scripts/wiring_guard.py` の除外を個別の名前から「"." で始まる directory 全て」に変更 (追跡外の作業用 directory を拾わない)

### Changed — CI が ALICE-* sibling を固定 commit で checkout する (2026-10-12)

CI は sibling (ALICE-SDF / ALICE-LOL など) の main をそのまま checkout していたため、sibling 側で版を切り替えている途中
(alice-zip 0.10 と、それを 0.9 で要求する alice-lol が同時に main にある期間) に `cargo` の解決が失敗し、再現もできなかった
- `.github/siblings.toml` に sibling ごとの commit SHA を置き、`.github/actions/alice-siblings` がその commit を checkout する
  5 workflow (ci / fuzz / quality-deep / release / security-audit) の 8 job の重複していた checkout をこの action 1 つにまとめた
- action は pin を検査器と同じ script (`--emit-pins`、`tomllib`) で読み、最後の step で checkout した HEAD を全 pin について照合する
  (`--heads-only`) ⇒ sibling を checkout する全 job で、固定 commit 以外を取ったら fail する  siblings.toml の各行は
  `ALICE-Xxx = "<40 桁の SHA>"` の 1 通りの綴りに限る
- workflow は許可リストで検査する: `repository:` を書かない (sibling は composite action だけが checkout する) / repo を
  取得する command (git clone・fetch・pull・submodule・remote・archive、gh repo clone、gh api、svn) を使わない /
  GitHub から download する URL は literal で ALICE の名前を含めない / `owner/ALICE-Xxx` 形の参照は大文字小文字を問わず不可
  (`../ALICE-Xxx` と本 repo の action の path は可)  見た step が 0 件なら fail
- sibling を使う全 job の最初 (action の最後) と最後に、各 sibling の中身が固定 commit の tree と完全一致することを照合する
  (HEAD / `git diff --quiet HEAD` / `git status --porcelain --untracked-files=all --ignored=matching` が空、例外は `target/`
  のみ / index の skip flag・変更無視 flag で隠した file と replace ref も不可)  取得の方法 (clone / tarball / script) に
  依らず、cargo が読む bytes が固定 commit と違えば fail する  workflow の静的な許可リストは早期の検出として残す
- 同じ step で `cargo metadata --locked` の解決結果も照合する: path の alice-* は固定した sibling の directory の下から来ること
  / 固定した sibling が提供する名前が registry・git から来ていないこと / registry の alice-* を `[patch]` で手元の path に
  差し替えていないこと (sibling の tree の外での差し替えを捕まえる)  照合した alice-* が 0 件なら fail
- 解決先の照合で `cargo metadata` を repo の root で実行する (cargo は `.cargo/config.toml` を実行した directory とその親から
  探すので、workspace の root から実行すると repo の中の config の `paths` / `[patch]` を見落としていた)
- `scripts/ci_sibling_checkout_check.py` を追加 (ci.yml の Linux job と preflight)  `cargo metadata` の出力から実際に読む
  sibling を求め、siblings.toml との過不足 / action との不一致 / workflow での直接 checkout / checkout された HEAD と固定 commit の
  不一致 / 解決の失敗を fail にする  比較対象が 0 件のときも fail
- 必要な sibling は SDF / LOL / Zip / LLM / Physics / Bamboo / Print の 7 つで、以前 checkout していた ALICE-View と
  ALICE-Kinematics は不要だった
- `cargo fmt --all` を `cargo fmt` に変更 (`--all` は path 依存の sibling まで 1800 file 超を 1 回の rustfmt に渡し、
  Windows の引数長の上限 (os error 206) に当たっていた sibling の fmt は各 repo の CI が見る)
- `scripts/fmt_scope_check.py` を追加: rustfmt が検査した file の集合が workspace member の追跡済み `.rs` 全部 (現在 50) と
  一致することを確かめる (漏れ・0 件は fail)  `--all` を外しても本 repo の検査範囲は変わらない
- 固定した組で `Cargo.lock` を更新: alice-sdf 4.0.0 → 5.1.1 / alice-physics 1.4.0 → 2.3.0 / alice-det-math 0.3.1 → 0.4.0 /
  alice-lol-macro 0.2.0 → 0.2.1 / alice-zip 0.9.0 を追加
- 固定 commit の更新は 1 つずつでなく組ごとに行う (全 SHA の差し替え → 解決 → build / test → `Cargo.lock` の更新)

### Changed — 回転した薄板の tight AABB が実寸に絞られる (ALICE-SDF `08582d5` の追従) (2026-10-03)

ALICE-SDF の `compute_tight_aabb` が準位集合の bound を伝播するようになり、回転した薄板 (X 軸 45 度の 30 x 30 x 0.8 mm) の AABB が
半幅 15 x 500 x 500 (探索範囲の上限) から実寸 15 x 11 x 11 に絞られた 再メッシュ経路 (実測 AABB との比較) に入る形状が減る
- `repair_mesh_never_adds_boundary_edges` の 1 回目の bounds を tight AABB から作らず、膨らんでいた頃の値で固定した (tight AABB が絞られると `repair_all` が水密な mesh を開く入力が再現しなくなるため)
- `tight_aabb_of_a_rotated_thin_plate_is_no_longer_inflated` を追加 (0 / 15 / 30 / 45 / 60 / 75 / 90 度で膨張判定に掛からず、板を切り落とさない)

### Fixed — 離れた小部品が 3MF から落ちる / bbox が長いと薄板を取りこぼす 2 件 (2026-10-02)

薄板 (30 x 30 x 0.8 mm = 720 mm³) と、120mm 離れた球 (半径 3mm = 113.1 mm³) の合計 833.1 mm³ が、エラー無しで次のように壊れていた

- 回転あり: 体積 715.9 (球が脱落)  1 回目の粗い mesh が離れた球を捉えず、その実測 AABB に球が入らないので 2 回目の bounds にも入らなかった
- 回転なし: 体積 530.9・境界エッジ 96  bbox が 30 x 138 x 6 mm と長く、bbox 全体に張った単一の格子 (cell 約 1.4mm) が 0.8mm の板を取りこぼしていた

`mesh_with_remesh` が、mesh の実測ではなく区間評価 (`alice_sdf::interval::eval_interval`) の八分木で固体の存在しうる領域を
**証明** する (`f > 0` が箱の全域で成り立つ箱を排除、`f < 0` の箱は分割せず採用、NaN は排除しない)  残った単位を 26 近傍の
連結成分に分け、成分の AABB が領域より十分小さければその AABB で再探索して絞り込み、padding 込みで重なる AABB は 1 つにまとめる
成分ごとに専用の格子でメッシュして連結する  単一成分で tight AABB が膨らんでいない通常の形状は従来の経路のままで出力は変わらない
評価回数の予算 (40 万回) を超えたとき、成分が 8 個を超えるときは従来の実測方式に退避する
- 体積誤差 3% 以内・水密を e2e で検査、成分 AABB の健全性 (閉形式の板の内部の点を全て含む) と tight さ、成分数、26 近傍、統合、
  index offset、予算、NaN を単体で検査した 変異 11 種と配線変異 1 種が red になることを実測した
- 八分木の費用は debug ビルドで 1 回 110〜320 ms (preview / export / `safety_check_lol` の各経路で 1 回)

### Fixed — X 軸回転した薄板の 3MF が体積を失う / 水密が破れる 2 件 (2026-10-02)

`rotate(θ, 0, 0, box3d(15, 15, 0.4))` (正解体積 720 mm³、回転で不変) の 3MF が、エラー無しで次のように壊れていた

- **30 / 60 度で体積の 73% を失う**: tight AABB が回転で 612 x 578 mm に膨張し、1 回目の mesh が薄板をほとんど捉えられず、
  その実測 AABB (y ±0.40 / z ±0.27、真値は y ±13.2 / z ±7.85) ± 5mm の bounds で 2 回目の再メッシュが板を切り落としていた
  再メッシュを `mesh_with_remesh` に共通化し、mesh が bounds の 1 cell 以内に接している (= 切り落とされている) 間は
  bounds を倍々に広げる (上限は 1 回目の bounds) 上限まで広げても切り落としが残るときは欠損した mesh を返さずエラーにする
  preview / 3MF export / `safety_check_lol` の 3 経路が同じ helper を使う
- **45 度 (Preview) で水密な mesh が開く**: `MeshRepair::repair_all` が 845 三角形を落として境界エッジ 567 本を作っていた
  `repair_mesh` で、修復が境界エッジを増やしたら修復前の mesh を採用する
- 体積誤差は最大 0.8% (Preview) / 0.2% (High)、全角度で境界エッジ 0 (15 / 30 / 45 / 60 度、Preview / High)
- 変異 (広げない / 拡張量 0 / 上側の検出を外す / 反復上限 0 / 水密ガードを外す / 境界の数え方) が red になることを実測した

### Added — 2 回目の再メッシュ判定 (`compute_empirical_aabb` / `aabb_significantly_inflated`) の oracle (2026-10-02)

仕様 (頂点位置の軸ごとの min / max、空 mesh は `None`、いずれかの軸で tight / empirical が厳密に 2.0 を超えたら再メッシュ、
empirical の 0 厚軸は 1e-3 に floor) を境界値で検査する 7 本を `crates/core/src/pipeline.rs` の test に追加した
独立の軸ごと fold との突合、頂点順の入れ替え、比がちょうど 2.0 と f32 で次の値の対、単一軸のみの膨らみ、0 厚軸を含む
変異 6 種 (`>` を `>=`、閾値 3.0、z 軸を見ない、floor を外す、max を更新しない、原点で初期化) が red になることを実測した

### Added — DC 経路 (薄板) と MC 経路の体積・水密を閉形式で突合する oracle (2026-10-02)

薄板 `box3d(15, 15, 0.4)` (30 x 30 x 0.8 mm、DC 経路) の 3MF を独立パーサで読み戻し、体積が
箱の閉形式 `30 * 30 * 0.8` と相対誤差 5e-4 以内で一致し、全エッジがちょうど 2 枚に共有される (水密) ことを
`crates/core/tests/e2e_pipeline.rs` で検査する 対照として厚い箱 (MC 経路) も同様に突合する
薄板は t2p の主力形状だが、DC 経路の実 mesh はこれまで体積も水密も突合されていなかった
変異 (DC 判定を MC に回す / DC の分解能 / bounds の縮小 / MC の分解能) が red になることを実測した

### Added — 配線ガード (`scripts/wiring_guard.py`) を導入 (2026-10-02)

実装したが production から呼ばれていない `pub` / `pub(crate)` item と、理由の無い
`#[allow(dead_code)]` の新規追加を CI で止める検査器を ALICE-Physics から移植した
対象は workspace member の `crates/app` / `core` / `llm` / `network`

- `scripts/test_wiring_guard.py`: 検査器自身の oracle 79 本
- `scripts/wiring-baseline.txt`: 既存の違反 (unwired 146 件 / dead_code 7 件) を記録するラチェット 既存分は解消しておらず、新規の違反だけが fail する
- CI: `wiring-guard` job (ubuntu / macOS / Windows) と `scripts/preflight.sh` の step を追加
- 限界: workspace から exclude された `crates/worker` の pub item は unwired の走査対象外 (dead_code は走査される)

## [v0.1.0-beta.4.1] - 2026-09-28

肉厚と連結性の判定を証明ベースに移した hardening release (STEP export / 連結性判定 / integration test 未実行 の 3 件を修正)

### Added
- **証明の三値を share payload に載せた** (`quality.printability`、schema v1 に optional 追加) `erosion` / `connectivity` / `thin_triangles` / `min_local_thickness_mm` / `notes` を上げる `safety_violations` は未決定を表現できない (未決定は violation message を出さない) ので、受け手の LoRA 学習が「検証していない形状」を「合格した形状」として扱わないよう三値のまま渡す `quality.required` には入れないため、proof pass を通らない export 経路と 2026-09-28 より前の client が書いた payload も従来通り受ける (`ok` は送らない — bool 1 個に畳むと未決定と違反の区別が失われる)
- **判定の oracle 7 本** (`pipeline.rs` / `network/src/share.rs` / `network/tests/share_schema.rs`) — 薄板 2 枚の分離検出 / 1 枚板と 5mm 殻の決着 / **fail が 1 件も出ない形状 (肉厚 2.0mm の殻) で `ok` が false になること** / 三値のうち `proved` だけが合格であること / 三値が wire を往復して畳まれないこと / 未知の verdict を schema が弾くこと / printability 無しでも schema conform すること 実測した境界は 2.5mm → `proved` / 2.0mm → `undecided` で fail 0 件 / 1.6mm → 薄い三角形 59,024 枚で `violated`
- **`actionlint` job** (`ci.yml`) — 上記 node20 逸脱を push 時点で検知できていなかったので追加 (canonical: ALICE-LOL / ALICE-SDF)
- **`quality-deep.yml`** (`cargo-mutants`、canonical: ALICE-Physics / ALICE-LOL) — 判定の核 (`printability_summary` / `connectivity_verdict` / `should_use_dual_contouring` / `from_message` 等) の**検出力**を測り、生存変異 0 を gate にする 週次 + 該当 file を触った PR + 手動 測定結果 file が無い / 形式が想定外なら fail (「見つからない」を「問題なし」に読み替えない)
- **DfAM 測定 + 判定を 3MF export 経路に統合** (`alice_bamboo::dfam`、text-to-cad `dfam-check` 吸収) `MeshStats.dfam_summary` に壁厚 p05 / 最小穴径 / 最大ブリッジ span / サポート面積比 / 単位疑義 / watertight の findings (FDM 限界、ISO/ASTM 52910 準拠の保守値) Generate 画面に「DfAM (FDM): OK/NG …」行 + pass 以外の finding を表示
- **LLM retry loop に DfAM 違反を接続** `safety_check_lol` が Preview 解像度 mesh で DfAM を測り、`DfAM wall thickness / positive feature / hole diameter` の Fail を `fix_prompt` に流す (`SafetyViolationKind::{WallTooThin, FeatureTooSmall, HoleTooSmall, BridgeTooLong, NotWatertight}` 追加) bridge / watertight は retry trigger にしない (曲面形状で常時発火 / mesher の性質で LOL 設計の問題でないため)、UI + manifest 表示のみ
- manifest `safety_violations` に DfAM Fail メッセージを merge (LoRA 学習データの品質シグナル)
- **造形向き探索** (`alice_bamboo::dfam::evaluate_orientations`、軸整列 6 + 球面 32 候補、mesh を回さず造形軸を回す) 現在よりサポート面積が 20 %+ 減る向きがあれば `DfamSummary.orientation_hint` に `"rotate: -Z up → support 312 → 0 mm² (-100%), height 10.0 mm"` を格納し Generate 画面に表示
- **`ttp` headless CLI** (`crates/core/src/bin/ttp.rs`、clap 不使用) `ttp check --lol` (parse + safety + DfAM、JSON) / `ttp export --lol --out --format 3mf|stl|fbx|step|gcode --quality` / `ttp validate --gcode --bed h2d|h2d-dual|x1c|a1-mini` (alice_print 静的検証)、exit 0 / 1 / 3 (findings fail)
- **Agent Skill** `skills/text-to-print/SKILL.md` (+ `references/lol-dsl-quickref.md` = system_prompt mirror、`references/dfam-findings.md`) + `.claude-plugin/plugin.json` / `marketplace.json` — Claude Code / Codex から `ttp` 経由で LOL 作成 → DfAM check → export → validate を回せる (text-to-cad の配布形態を吸収、Free tier の入口)

### Changed
- **肉厚と連結性の判定を証明ベースに移した** (`alice_sdf::validity` + `alice_bamboo::law`、新 `PrintabilitySummary`) DfAM (`alice_bamboo::dfam`) の壁厚は mesh 頂点からのレイキャスト標本 p05 なので、標本の隙間にある薄壁を「見つからなかった = 合格」にできる 同じ量を 2 つの source で判定すると緩い側に倒れるため canonical source を 1 つに寄せた
  - 肉厚: erosion (`Round { radius: -t/2 }` は SDF では厳密) の区間演算による**証明** + 三角形ごとの内向き march による**厳密な**局所肉厚 三値 (`proved` / `violated` / `undecided`) で、`undecided` を合格に繰り上げない
  - 連結性: `Constraint::Reachable` で「最も離れた内部 2 点が繋がっているか」= **2 つに分かれた造形物の検出** (印刷すると分解する) 解像度依存なので 16 → 32 と上げて未決定が消えるかを見る (ALICE-LOL 実装者の助言、2026-09-28)
  - retry loop: `DfAM wall thickness` を retry trigger から外し、`Printability wall thickness:` / `Printability connectivity:` を追加 `SafetyViolationKind::Disconnected` 新設 (directive = union で 1mm 以上重ねる / 連結バーを足す)
  - UI (Generate 画面) / `ttp check` / `ttp export` JSON / manifest `safety_violations` に `printability` を追加 overhang 角は閉形式で記録するが判定には使わない (曲面形状で常時発火するため)
- **CI: `alice-stubs` (manifest-only stub) を廃止して実 sibling checkout に統一** (`security-audit.yml` 4 job / `fuzz.yml`) stub は AGPL の `alice-bamboo` / `alice-physics` / `alice-llm` を `license = "MIT OR Apache-2.0"` の空 crate として申告していたので、`deny.toml` の `licenses.exceptions` 4 件が **CI では一度も評価されず**、`alice-print` / `alice-lol` とその transitive 依存も dep 木から消えていた (ALICE-LOL `a6ba9f7` / Physics `3472768` と同じ置換)
- **CI: node20 runtime の action を更新** — `actions/upload-artifact@v4` → v6 (6 箇所) / `download-artifact@v4` → v7 / `softprops/action-gh-release@v2` → v3 GitHub の node20 削除 (2026-09-23) で release workflow が落ちる前に追従 sibling は既に対応済 (SDF `ea09e88` / LOL `fbcc63a` / Bamboo `433ff59`)
- **e2e_pipeline の assert を解析解 oracle に格上げ** 「ZIP として開ける / `<model` を含む」だけでなく、Bambu production extension の `p:path` 参照を辿って `3D/Objects/*.model` の mesh を読み戻し、(1) 頂点・三角形数が `MeshStats` と一致 (2) 符号付き体積が `sphere(10)` の 4/3πr³ と ±5% 一致 (3) 全無向エッジが 2 枚共有 = 水密 を検証 参照先 part の不在も検出する (Bambu Studio が開けない 3MF の早期検出)
- **LOL grammar の copy 運用を廃止** `crates/llm/src/lol.gbnf` (手動 copy) を削除し、`grammar_lol::LOL_GBNF` は `alice_bamboo::LOL_GBNF` (= `alice_lol::LOL_GBNF`、feature 外 `include_str!`) の re-export に copy は ALICE-LOL 本体から 199 行 drift しており (comment / whitespace 厳格化、`program(...)` Intent wrapper なし)、逆に copy 側だけに product shortcut 65 個が足されて本体に upstream されていなかった (mechanical archetype 38 個はどちらにも無く、system prompt が案内するのに grammar ON だと emit 不能) → ALICE-LOL 側で 103 構文を canonical grammar に追加 + parser ⊆ grammar の drift guard test (`52e9834`) `enforce_lol_grammar` は default OFF のまま (iGPU での速度のため) `crates/llm` に `alice-bamboo` 依存追加
- **`system_prompt.md`**: 「NO `//` comments, NO indent: max 1 space between tokens」を追記 (canonical grammar が comment / 連続 whitespace を拒否するため)、example の indent 除去、reminder 2 行を短縮して 4488 chars (4500 予算内)
- `ROADMAP.md` P1-10 を実態に同期 (G-code export 配線済、ベッド配置変換 fix 反映)

### Fixed
- **連結性の判定が実際には走っていなかった** (`printability_summary` の端点選び、2026-09-28 実測) 内部点を「bound 全体のセル**対角**より深い格子点」という等方の距離基準で選んでいたため、Z 厚 0.8mm の板でも**肉厚 5mm の球殻でも**端点が 0 個になり `not_run` で素通りしていた t2p の主力形状は薄物 (DC 経路 = 5mm 以下、SKADIS panel / coin) なので、2 つに分かれた造形物の検出が主力形状で機能していない状態だった 端点を判定器と同じ基準 (`alice_sdf::interval::eval_interval` でセル全体が内部と確定したセルの中心、非等方セルでもそのまま効く) で選ぶよう変更し、薄板 2 枚の分離が `violated`、1 枚板と 5mm 殻が `proved` に決着することを oracle で固定 判定時間も短縮 (該当 test 3.55s → 1.77s)
- **STEP export が STEP として成立していなかった** (sibling `alice-sdf` `762b04c` で修正、本 repo は path dep なので追従のみ) 未定義の `#0` を参照する file を書いていたため、**v0.1.0-beta.4 まで `ttp export --format step` / UI の STEP 出力はどの CAD でも開けなかった** 併せて `crates/core/tests/e2e_pipeline.rs` に t2p 側の回帰 test を追加 (1 回の export に 3 つの oracle: 未定義参照 0 件 / AP214 の必須 root 4 つ / **頂点が半径 10mm の球面に乗る** — 独立実装 (python 再パース) の実測 max |r − 10| = 0.0006mm に対し許容 0.05mm) detector が vacuous でないことを確かめる注入 test も別に置いた
- **`Cargo.lock` が sibling の現状から遅れていた** — `alice-physics` が `alice-det-math` 0.3.1 に上がっているのを取り込み (`alice-sdf` は 0.2.0 のままなので 2 version 併存、`bans.multiple-versions = "warn"`) `fuzz/Cargo.lock` も同期
- **`.msi` の ProductVersion が多段 pre-release tag で生成できなかった** (`release.yml`、2026-09-28 実測) `cargo wix --install-version` は値を semver crate で parse し、**pre-release の数値成分が 1 個**のものだけを MSI の 4 field 目に変換する 制約の両側を実測で確認: `0.1.0-beta.4.1` は数値が 2 個で `couldn't convert the prerelease components to an integer` (run 36418654823)、自分で 4 field を作った `0.1.0.401` は `Error[7] (Version) unexpected character '.' after patch version number` (run 36422468210) 既存の guard は `^\d+\.\d+\.\d+` の**前方一致**で core しか見ておらず pre-release の map 可能性を検査していなかった (else 側の `$version = "0.0.0"` も silent fallback) tag 文字列をそのまま渡すのをやめ、**数値成分を base-100 で 1 個に畳んだ SemVer** を渡して 4 field 化は cargo-wix に任せる形に変更 (`beta.4` → `beta.400` / `beta.4.1` → `beta.401` / `beta.5` → `beta.500` / `rc.1.2` → `rc.102`、非数値成分は順序を保持、数値は 2 個まで各 99 以下) map 不能なら `exit 1` で fail fast、GA tag は従来通り core をそのまま渡す 併せて `workflow_dispatch` (dry run) は `ref_type` が tag でないため `Cargo.toml` の `workspace.package.version` を source にした 規則は cargo-wix 側の変換も模した同一移植 script で 22 case 検証済 (Windows Installer が 4 field 目を upgrade 比較で無視する既知制約は変えない)
- **CI / preflight が integration test を一度も走らせていなかった** test step が `cargo test --workspace --lib` だったため `crates/core/tests/e2e_pipeline.rs` (text → LOL → mesh → 3MF の唯一の e2e) と `crates/network/tests/share_schema.rs` は compile されるだけで実行されていなかった `--workspace --all-targets` に変更し、`scripts/preflight.sh` も再生成して local gate に含めた

### Security
- rustls 0.23.43 → 0.23.45 (RUSTSEC-2026-0285、TLS 1.3 handshake message encryption level 境界) `cargo update -p rustls`
- `deny.toml` を real sibling 依存木で green に: ALICE-* sibling 4 crate (alice-physics / alice-llm AGPL、alice-bamboo AGPL-3.0-only、alice-print proprietary) を `licenses.exceptions` で限定許可、bincode 1.3 unmaintained (RUSTSEC-2025-0141、`.cargo/audit.toml` と同期) を ignore
- `scripts/preflight.sh` 追加 (CI gate の local 逐語再現、pre-push hook が `--quick` で block)

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
  `contact@extoria.co.jp` (法人窓口) 5 hit
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

初回 β release 7 artifact 公開 ([GitHub Releases v0.1.0](https://github.com/ext-sakamoro/text-to-print/releases))

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
