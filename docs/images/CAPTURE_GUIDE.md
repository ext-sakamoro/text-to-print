# Screenshots / GIF (β release visual assets)

このディレクトリは README hero + GitHub Release page + 将来の Zenn / Twitter 記事で使う画像・動画素材を格納する user 側で macOS 標準の screen capture ツールで撮影し、以下 file 名で置く

## Ja / En 2 版撮影ルール (P1-11 対応、2026-09-07)

英語 UI が landing 済 (Phase 2-5) なので、README §Screenshots は Ja/En 2 grid で見せる 各 shot を **日本語 UI + 英語 UI の 2 版** で撮る:

- 日本語 UI 版: file 名は下表の canonical (`hero.png` 等)
- 英語 UI 版: 同名に `-en` suffix (`hero-en.png` / `template-section-en.png` / `customizer-en.png` / `llm-generation-en.png` / `bambu-import-en.png` / `gallery-share-en.png`) `print-result.jpg` (実プリント) は言語共通で 1 shot のみ

### 英語 UI mode での起動方法 (どちらか)

- **Settings UI 経由**: Settings tab → Language 展開 → 「English」選択 (即時切替、DB profile.lang_pref に永続化)
- **env 変数**: 起動時に `APP_LANG=en cargo run --release --package text-to-print` (or `TEXT_TO_PRINT_LANG=en`) Ja user が macOS system locale ja のまま英語 UI を撮影する時に便利

env 経由は起動時 1 回のみ評価 (`OnceLock` cache)、Settings UI 経由は毎 frame 再評価で live 反映

## 撮影 checklist (12 screenshot = 6 shot × Ja/En + 1 optional GIF + 1 print result)

| # | file name (Ja / En) | 内容 | 推奨サイズ | 撮影ツール |
|--|--|--|--|--|
| 1 | `hero.png` / `hero-en.png` | App 起動画面の全景 (左 pane に生成 UI、右 pane に 3D preview mesh、window 全体) | 2400×1500 or Retina 撮影 | Command+Shift+4 → Space → window click |
| 2 | `template-section.png` / `template-section-en.png` | 生成 tab の「テンプレート / Templates」collapsing 展開状態 preset button 群 + 上部 small text `presets: cache / version 2026-08-...` が写ってる | 1400×900 | Command+Shift+4 → 領域選択 |
| 3 | `customizer.png` / `customizer-en.png` | 「カスタマイザー / Customizer」collapsing 展開、Gridfinity or SKADIS panel の slider 操作中 (「作成 / Create: ...mm」ラベル + 「生成 / Generate」button 見える) | 1400×900 | 同上 |
| 4 | `llm-generation.png` / `llm-generation-en.png` | 自然言語 prompt 入力 → LLM 生成中 (LLM/parse/mesh/safety/export の phase progress bar が動いてる瞬間 or 100% 到達直後、生成 mesh preview あり) | 2400×1500 | 生成 button 押した直後に Command+Shift+4 |
| 5 | `bambu-import.png` / `bambu-import-en.png` | 出力した 3MF を Bambu Studio で開いた view (mesh の色付き、AMS filament assignment 見える) | 1600×1000 | Bambu Studio window scoped capture |
| 6 | `gallery-share.png` / `gallery-share-en.png` | Gallery share confirm dialog (公開確認 modal 表示中、Free tier 生成完了直後) | 1400×900 | Command+Shift+4 |
| 7 | `print-result.jpg` (optional、言語共通) | 実際の 3D 印刷物の写真 (SKADIS panel or Gridfinity bin、Bambu H2D 出力品) | 実物撮影 4:3 | iPhone 等 |

## GIF (Level 2、~20s workflow) — **2026-09-07 skip 決定、post-β 検討候補**

β release では **静止画 6 shot で十分** と判断 (hero + template-section +
customizer + llm-generation + bambu-import + gallery-share bonus) 動画は
post-β 追加検討 (Ko-fi tutorial や Zenn 記事執筆時に追加撮影)

以下 spec は参考として残置 撮影する時は再開始点として使う

| # | file name | 内容 | 変換前 | 変換後 |
|--|--|--|--|--|
| G1 | `hero.gif` | text prompt → LLM 生成 → 3D preview → export → Bambu Studio open の end-to-end 20s sequence | `hero.mov` (macOS Command+Shift+5 で screen record、5-30s 適宜 trim) | `hero.gif` (fps 15、幅 1200px、2-5 MB target) |

### mov → GIF 変換 command (私が実行)

```bash
# 1. trim (optional、start/duration 秒指定)
ffmpeg -i docs/images/hero.mov -ss 00:00:00 -t 20 -c copy /tmp/hero-trim.mov

# 2. palette 生成 + GIF 変換 (高品質)
ffmpeg -i /tmp/hero-trim.mov -vf "fps=15,scale=1200:-1:flags=lanczos,palettegen" /tmp/palette.png
ffmpeg -i /tmp/hero-trim.mov -i /tmp/palette.png -filter_complex "fps=15,scale=1200:-1:flags=lanczos[x];[x][1:v]paletteuse" -loop 0 docs/images/hero.gif

# 3. size 確認 (2-5 MB target、超えたら fps 10 or scale 800px に下げる)
ls -lh docs/images/hero.gif
```

### GIF が大きい (>5 MB) 場合の再圧縮 option

```bash
# fps 10 に落とす
ffmpeg -i /tmp/hero-trim.mov -vf "fps=10,scale=1000:-1:flags=lanczos,palettegen" /tmp/palette.png
ffmpeg -i /tmp/hero-trim.mov -i /tmp/palette.png -filter_complex "fps=10,scale=1000:-1:flags=lanczos[x];[x][1:v]paletteuse" -loop 0 docs/images/hero.gif

# gifsicle で lossy 圧縮 (追加削減)
gifsicle -O3 --lossy=80 -o docs/images/hero-optimized.gif docs/images/hero.gif
```

## 撮影時の注意

- **個人情報 masking**: window title の user 名 (`ys@wainoMacBook-Air` 等)、Finder 側の path、開いてる別 tab の company name 等は capture 領域から外す
- **light theme 統一**: dark mode / light mode 混在すると README 見た目がバラバラ 撮影時は macOS System Settings → Appearance で light に統一 (または全部 dark に統一)
- **解像度**: Retina display 2x scale 撮影推奨 (2400×1500 相当)、README では GitHub が自動で responsive scale する
- **filename 統一**: 上記 exact な file 名で置く README の埋込 markdown が壊れない

## 私 (Claude) 側の作業 (user 撮影後)

1. `docs/images/*.png` (Ja + En 12 shot) / `docs/images/hero.mov` を confirm
2. mov → gif 変換 (`ffmpeg` command 上記)
3. README の hero + Screenshots section の Ja/En 2 grid image embed を verify
4. GitHub Release page body update (`gh release edit v0.1.0 --notes "..."` で hero.png + hero.gif 埋込)
5. commit + push

現時点 scaffold は README 側 image embed spot 用意済 撮影完了 & filename 通り置いたら私が最終仕上げ実行 英語版 file が未着でも README の En section の image は broken link 表示になるだけ (Ja section は即機能)
