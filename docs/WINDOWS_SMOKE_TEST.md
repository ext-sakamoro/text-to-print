# Windows smoke test 手順 (β release、user 側検証用)

macOS で verified 済 Windows PC で以下 checklist を実行して README の
Platform verification status を更新する

## 前提

- Windows 10 (build 19041+) or Windows 11
- x64 arch (arm64 Windows は未対応)
- 空き disk 500 MB 以上
- Internet 接続 (初回 alice-llm-server sidecar 起動時に GGUF 自動 DL、~2 GB)

## Step 1: Install (`.msi` 経由推奨)

1. https://github.com/ext-sakamoro/text-to-print/releases/tag/v0.1.0 から
   `text-to-print-x86_64-pc-windows-msvc.msi` DL
2. double-click で install wizard 起動
3. **SmartScreen 警告** 出たら「詳細情報」→「実行」 (β 期間 unsigned)
4. install 完了、スタートメニューに `text-to-print` 追加確認

### 代替: `.zip` 経由 (install 不要、portable)

1. `text-to-print-x86_64-pc-windows-msvc.zip` DL
2. 適当なフォルダに展開 (例: `C:\Users\<name>\text-to-print\`)
3. `text-to-print.exe` を double-click 起動

## Step 2: 起動 smoke test

- [ ] App window 起動 (5 tab: 生成 / ギャラリー / 履歴 / 設定 / 情報)
- [ ] 生成 tab で「テンプレート」collapsing 展開 preset 一覧表示
- [ ] SKADIS パネル 300×300 (or 適当な preset) click → mesh 生成完了 (~5-15 秒)
- [ ] 右 pane に 3D mesh preview 表示 (drag で orbit / scroll で zoom)
- [ ] 「保存...」button で `.3mf` export → Explorer で file 存在確認
- [ ] .3mf を Bambu Studio (or Prusa Slicer) で開く → mesh 表示確認

## Step 3: LLM 経由 smoke (option)

**local Qwen 3B (default)**:
- [ ] 実験機能 collapsing 展開
- [ ] prompt 入力 (例: `直径 30mm、高さ 40mm の円柱`)
- [ ] 「生成 (LLM)」click → **初回は GGUF DL で ~2 GB 待ち**
- [ ] 生成完了 → mesh preview 表示

**BYO LLM (Gemini 推奨、無料枠)**:
- [ ] 設定 tab → BYO LLM section
- [ ] Google Gemini 選択 → API key 入力 → 「保存」
- [ ] 生成 tab に戻る → prompt 入力 → 生成 (~2-5 秒で完了)

## Step 4: 結果報告

以下 template で GitHub Issues に報告 (bug でも「動きました」でも歓迎):

```markdown
## Windows smoke test 結果

- **OS**: Windows 11 Pro 23H2 (build XXXXX)
- **CPU**: (例: Intel Core i7-12700K / AMD Ryzen 7 5800X)
- **GPU**: (例: NVIDIA RTX 3060 / Intel Iris Xe)
- **RAM**: 16 GB
- **Install 方法**: .msi / .zip (どちらか)

### 動作結果

- [ ] Install 成功
- [ ] App 起動成功
- [ ] Template 生成 + preview 表示成功
- [ ] .3mf export + Bambu Studio 表示成功
- [ ] LLM 生成成功 (local Qwen / Gemini どちらか)

### 発生した問題 (あれば)

- 現象:
- log (Command Prompt で `text-to-print.exe --verbose > log.txt 2>&1` 等):

### 動作確認済 platform に追加してよい?
- [ ] はい (README §Platform verification status に反映)
```

## Known warnings (β 期間)

- SmartScreen「認識されないアプリ」 → Authenticode 未署名のため、Phase S3 (post-β) で対応
- MSI install 完了後の「システム変更を許可」dialog → 通常挙動、Yes で進行
- 初回起動時 Windows Defender scan → ~30 秒待機、以降通常起動

## 参考

- release page: https://github.com/ext-sakamoro/text-to-print/releases/tag/v0.1.0
- Platform verification status: [README §Install](../README.md#install-β-期間無署名)
- Bug report template: `.github/ISSUE_TEMPLATE/bug_report.md`
