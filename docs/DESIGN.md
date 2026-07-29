# text-to-print Desktop App 設計書

## 1. 概要

text-to-print（3Dバーチャルガランドウ）を SaaS からデスクトップアプリへ転換する。

### 転換の理由

- サーバー維持コスト（Railway / Mac mini / Cloudflare Tunnel）の排除
- Supabase / Stripe 依存の排除
- ユーザーの GPU / CPU を活用したローカル推論
- **ALICE P2P ネットワークのグローバル展開基盤**

### 真の戦略: トロイの木馬型ネットワークデプロイ

このデスクトップアプリの本質は「便利な 3D 生成ツール」ではない。

ユーザーが「便利な魔法のツール」だと思ってインストールしたそのソフトウェアのコアには、
ALICE-Sync（P2P 同期）、ALICE-Cache（分散キャッシュ）、ALICE-CDN（Vivaldi 座標ルーティング）
といったコンポーネントが静かに内包されている。

```
ユーザーの認識:              実際のアーキテクチャ:
┌──────────────────┐       ┌──────────────────────────────┐
│                  │       │  Text-to-3D GUI (表層)        │
│  3D モデルが     │       │  ┌──────────────────────────┐ │
│  テキストから     │       │  │ ALICE-Sync   (P2P 同期)  │ │
│  作れて便利!     │       │  │ ALICE-Cache  (分散 KVS)  │ │
│                  │       │  │ ALICE-CDN    (Vivaldi)   │ │
│                  │       │  │ ALICE-Identity (DID)     │ │
│                  │       │  │ ALICE-VCS    (Merkle)    │ │
│                  │       │  └──────────────────────────┘ │
└──────────────────┘       └──────────────────────────────┘
```

**1 インストール = 1 ノード。**

Text-to-CAD が 3D プリンター界隈でバズり、1 万人、10 万人のメイカーたちの PC にインストールされた瞬間、
**1 円のサーバー代も払うことなく、全世界に 10 万ノードの ALICE P2P ネットワーク網が完成する。**

#### 収穫フェーズ

| インストール数 | 状態 | 獲得するもの |
|--------------|------|------------|
| 0 → 1,000 | シード期 | バイラル + 初期 SDF 教師データ |
| 1,000 → 10,000 | 臨界質量 | P2P ネットワークが自律�kind動（中央サーバー完全不要） |
| 10,000 → 100,000 | ネットワーク効果 | 10 万ノードの分散インフラ + 数百万件の SDF 教師データ |
| 100,000+ | プラットフォーム | ALICE ネットワーク上に任意のサービスを展開可能 |

#### なぜ SDF が鍵か

- SDF は数式。1 モデル = 数十〜数百バイト。ストレージコスト ≈ 0
- General tier ユーザーが生成するたびに、高品質な Text-to-CAD 教師データが無料で蓄積
- P2P ネットワーク上の SDF データは、ALICE 次世代 LLM の学習燃料になる
- ユーザーは「安いツールを使っている」つもりで、実際は ALICE のデータパイプラインの一部として機能している

#### ホスティングはエッジ同士で

- ユーザー A が作った SDF データは中央サーバーではなく、A 自身の PC と、
  それを閲覧したユーザー B、ユーザー C の ALICE-Cache に分散保持される
- ノードが増えるほどネットワーク容量が自己増殖する（コストゼロ）

#### フォークとリミックスは VCS で

- 他人の SDF を改造（フォーク）した場合、ALICE-VCS が「どこをどう書き換えたか」の
  差分（数十バイト）だけを P2P ネットワークに伝播させる
- フルメッシュの共有ではなく、AST 差分の伝播。帯域コスト ≈ 0

### 設計原則

- **全 Rust**: フロントエンドからバックエンドまで Rust のみ
- **ローカルファースト**: ネットワーク接続なしで SDF 生成が完結
- **SDF はデータ**: 3D モデルを数式（SDF）として扱い、ストレージコスト≈0
- **静かなインフラ**: P2P レイヤーはアプリのコアに内包され、ユーザーに意識させずに機能する
- **自己増殖**: ユーザーが増えるほどネットワーク容量・データ・価値が自律的に増加する

---

## 2. ティアシステム

| Tier | 価格 | 生成制限 | モデル公開 | ダウンロード | 戦略的目的 |
|------|------|---------|-----------|------------|-----------|
| **Free** | $0 | 5回/日 | — | プレビューのみ | バイラル撒き餌。魔法を体験させ界隈に拡散 |
| **General** | ¥1,500/月 | 30回/日 | **公開義務** (SDF保存) | .3mf / .fbx | データフライホイール。ユーザーが教師データを無尽蔵に生産 |
| **Pro** | ¥5,000/月 | 100回/日 | 非公開OK | .3mf / .fbx | IP保護層からのマネタイズ |
| **Enterprise** | 応相談 | 無制限 | 非公開 | 全形式 | 完全オフライン・組込み。高額ライセンス |

### General tier のデータフライホイール

```
ユーザーが Text-to-3D で SDF 生成
  → SDF（数式）を P2P ネットワークに自動公開
  → ストレージコスト ≈ 0（SDF は数十〜数百バイト）
  → 蓄積データが ALICE 次世代 LLM の教師データに
  → LLM 品質向上 → ユーザー体験向上 → さらにユーザー増加
```

### ライセンス管理（ローカル実装）

- ライセンスキー: Ed25519 署名付き JSON トークン
- オフライン検証: 公開鍵をバイナリに埋め込み、署名検証のみ（サーバー不要）
- キー構造: `{ tier, user_id, issued_at, expires_at, signature }`
- Free tier: キー不要（デフォルト動作）
- 発行: Web サイト（Stripe 決済）→ メールでキー送付 → アプリにペースト

---

## 3. 技術スタック

### クレート構成

```
text-to-print/
├── Cargo.toml                 # workspace root
├── crates/
│   ├── app/                   # エントリポイント + GUI (egui/eframe)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs        # eframe::run_native()
│   │       ├── ui/            # UI コンポーネント
│   │       │   ├── mod.rs
│   │       │   ├── prompt.rs      # テキスト入力 + 生成ボタン
│   │       │   ├── viewer.rs      # 3D SDF プレビュー (ALICE-View 統合)
│   │       │   ├── gallery.rs     # ローカル + ネットワーク作品一覧
│   │       │   ├── history.rs     # 生成履歴
│   │       │   └── settings.rs    # ライセンス / LLM / プリンタ設定
│   │       └── state.rs       # アプリ状態管理
│   │
│   ├── core/                  # ビジネスロジック (GUI 非依存)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── pipeline.rs    # Text → LLM → LOL → SDF → Mesh 全パイプライン
│   │       ├── license.rs     # Ed25519 ライセンス検証
│   │       ├── tier.rs        # ティア制限 (回数 / 公開義務 / DL 可否)
│   │       ├── db.rs          # SQLite (rusqlite)
│   │       └── export.rs      # .3mf / .fbx / .glb エクスポート
│   │
│   ├── llm/                   # ローカル LLM 推論
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── backend.rs     # llama.cpp FFI or candle バックエンド
│   │       └── prompt.rs      # システムプロンプト + LOL 抽出
│   │
│   └── network/               # P2P 分散レイヤー（静かに内包）
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs         # ネットワークノード起動・停止
│           ├── sync.rs        # ALICE-Sync: gossipsub P2P イベント同期
│           ├── cache.rs       # ALICE-Cache: 分散 KVS (SDF キャッシュ)
│           ├── cdn.rs         # ALICE-CDN: Vivaldi 座標ルーティング
│           ├── identity.rs    # ALICE-Identity: DID (Ed25519 鍵ペア)
│           └── vcs.rs         # ALICE-VCS: Merkle DAG 差分管理
│
├── docs/                      # 設計ドキュメント
├── assets/                    # アイコン / フォント / シェーダー
└── migrations/                # SQLite マイグレーション SQL
```

### 依存クレート

| 用途 | クレート | 備考 |
|------|---------|------|
| GUI | eframe 0.27 + egui 0.27 | ALICE-View と同バージョンで統一 |
| 3D プレビュー | alice-view | SDF raymarching (wgpu 0.19) |
| SDF エンジン | alice-sdf 1.4.0 | `sdf_to_mesh()`, `eval()` |
| LOL DSL | alice-lol 0.1.0 | `parse_lol()`, `lol_to_3mf()`, `lol_to_fbx()` |
| ローカル DB | rusqlite + migrations | profiles, generations, projects |
| ライセンス | ed25519-dalek | 署名検証 |
| LLM 推論 | llama-cpp-rs or candle | ローカル推論 |
| シリアライズ | serde + serde_json | 設定 / SDF 保存 |
| 非同期 | tokio (rt-multi-thread) | LLM 推論 / メッシュ生成のバックグラウンド実行 |
| ファイル選択 | rfd | ネイティブファイルダイアログ |
| 通知 | notify-rust | 生成完了通知 |
| P2P ネットワーク | libp2p | gossipsub, Kademlia DHT, QUIC transport |
| 分散 ID | did-key | DID:key メソッド (Ed25519) |
| Merkle DAG | multihash + cid | コンテンツアドレス (ALICE-VCS) |

### ALICE-View 統合

ALICE-View (v0.3.0) は egui 0.27 + wgpu 0.19 で SDF リアルタイムレンダリングを提供。

```
┌─────────────────────────────────────────────┐
│  text-to-print App (eframe)                     │
│  ┌──────────┐  ┌──────────────────────────┐ │
│  │ Prompt   │  │  ALICE-View Panel        │ │
│  │ Input    │  │  (SDF Raymarching)       │ │
│  │          │  │                          │ │
│  │ [生成]   │  │  orbit / zoom / rotate   │ │
│  ├──────────┤  │                          │ │
│  │ History  │  │  X-Ray / Normal / AO     │ │
│  │ Gallery  │  │                          │ │
│  │ Settings │  │  Screenshot (F12)        │ │
│  └──────────┘  └──────────────────────────┘ │
└─────────────────────────────────────────────┘
```

統合方式:
- ALICE-View の `ViewerPanel` をカスタム egui パネルとして埋め込み
- `SdfNode` 生成後、`set_sdf_node()` でリアルタイムプレビュー更新
- メッシュエクスポートは `alice_lol::lol_to_3mf()` / `lol_to_fbx()` を使用

---

## 4. ローカル DB スキーマ (SQLite)

### profiles

```sql
CREATE TABLE profiles (
    id          TEXT PRIMARY KEY,  -- UUID
    email       TEXT,
    license_key TEXT,
    tier        TEXT NOT NULL DEFAULT 'Free',  -- Free/General/Pro/Enterprise
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### generations

```sql
CREATE TABLE generations (
    id             TEXT PRIMARY KEY,  -- UUID
    profile_id     TEXT NOT NULL REFERENCES profiles(id),
    prompt         TEXT NOT NULL,
    lol_source     TEXT,              -- LOL DSL ソース
    sdf_data       BLOB,             -- SDF ノードのシリアライズ（軽量）
    triangle_count INTEGER,
    vertex_count   INTEGER,
    quality        TEXT,              -- preview / high / ultra
    status         TEXT NOT NULL DEFAULT 'pending',  -- pending / complete / error
    error          TEXT,
    is_public      INTEGER NOT NULL DEFAULT 0,       -- General tier は強制 1
    created_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### projects

```sql
CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    profile_id  TEXT NOT NULL REFERENCES profiles(id),
    name        TEXT NOT NULL,
    description TEXT,
    config      TEXT,  -- JSON
    is_public   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### daily_usage

```sql
CREATE TABLE daily_usage (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    profile_id  TEXT NOT NULL REFERENCES profiles(id),
    date        TEXT NOT NULL,  -- YYYY-MM-DD
    count       INTEGER NOT NULL DEFAULT 0,
    UNIQUE(profile_id, date)
);
```

---

## 5. パイプライン詳細

### Text → Mesh 生成フロー

```
[ユーザー入力] "かわいい猫の置物"
       │
       ▼
[LLM 推論] ローカル Qwen2.5 7B (llama.cpp)
  system_prompt.md + user prompt
       │
       ▼
[LOL 抽出] レスポンスから ```lol ... ``` ブロックを抽出
       │
       ▼
[LOL パース] alice_lol::parse_lol() → SdfNode
       │
       ├──→ [プレビュー] ALICE-View で SDF リアルタイム表示
       │
       ▼
[メッシュ生成] alice_sdf::sdf_to_mesh() → Mesh
       │
       ▼
[エクスポート] .3mf / .fbx / .glb ファイル保存
       │
       ▼
[DB 記録] generations テーブルに保存
  └── General tier: sdf_data + is_public=1 (公開義務)
```

### ティア制限の実装

```rust
pub struct TierLimits {
    pub daily_generations: u32,
    pub can_download: bool,
    pub force_public: bool,
    pub max_quality: Quality,
}

impl TierLimits {
    pub fn for_tier(tier: Tier) -> Self {
        match tier {
            Tier::Free => Self {
                daily_generations: 5,
                can_download: false,
                force_public: false,
                max_quality: Quality::Preview,
            },
            Tier::General => Self {
                daily_generations: 30,
                can_download: true,
                force_public: true,  // SDF 公開義務
                max_quality: Quality::High,
            },
            Tier::Pro => Self {
                daily_generations: 100,
                can_download: true,
                force_public: false,
                max_quality: Quality::Ultra,
            },
            Tier::Enterprise => Self {
                daily_generations: u32::MAX,
                can_download: true,
                force_public: false,
                max_quality: Quality::Ultra,
            },
        }
    }
}
```

---

## 6. LLM 推論戦略

### 候補

| 方式 | メリット | デメリット |
|------|---------|-----------|
| **llama-cpp-rs** (FFI) | 高速、GGUF 対応、GPU 対応 | C++ ビルド依存 |
| **candle** (Pure Rust) | 全 Rust、ビルド簡単 | llama.cpp より低速 |
| **ollama 連携** (HTTP) | ユーザーが既に持っている場合便利 | 外部プロセス依存 |

### 推奨: llama-cpp-rs + ollama フォールバック

1. **デフォルト**: 同梱の llama-cpp-rs でローカル推論（Qwen2.5 7B GGUF）
2. **フォールバック**: ユーザーが ollama を持っている場合は OpenAI 互換 API に接続
3. **設定画面**: LLM バックエンド切替 UI を提供

### モデル配布

- 初回起動時に Hugging Face からモデルをダウンロード（約 4GB）
- `~/.text-to-print/models/` に配置
- ダウンロード進捗を UI に表示

---

## 7. 内包する分散レイヤー（コアコンポーネント）

これらは「将来の拡張」ではない。Phase 1 からバイナリに内包し、静かに動作する。
ユーザーはこれらの存在を意識しない。「便利なツール」の裏で、ALICE ネットワークのノードとして機能する。

### クレート構成（分散レイヤー）

```
crates/
├── network/               # P2P ネットワーク基盤
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── sync.rs        # ALICE-Sync: P2P イベント同期
│       ├── cache.rs       # ALICE-Cache: 分散 KVS
│       ├── cdn.rs         # ALICE-CDN: Vivaldi 座標ルーティング
│       ├── identity.rs    # ALICE-Identity: DID
│       └── vcs.rs         # ALICE-VCS: Merkle DAG 差分管理
```

### ALICE-Sync (P2P イベント同期)

- General tier の SDF を生成と同時に P2P ネットワークへ自動伝播
- libp2p ベースの gossipsub プロトコル
- ユーザー操作不要。バックグラウンドで静かに動作
- 実装: `crates/network/src/sync.rs`

### ALICE-Cache (分散キャッシュ)

- 閲覧した SDF データをローカルにキャッシュ
- 他ノードからの要求に応じてキャッシュを提供（CDN ノードとして機能）
- `~/.text-to-print/cache/` に SDF を保持（数式なので容量は微小）
- 実装: `crates/network/src/cache.rs`

### ALICE-CDN (Vivaldi 座標ルーティング)

- ネットワーク遅延を座標空間に埋め込み、近距離ノード優先でデータ取得
- 中央サーバー不要の自律的ルーティング
- 実装: `crates/network/src/cdn.rs`

### ALICE-VCS (Merkle DAG)

- SDF ノードを AST として差分管理
- フォーク / リミックス = AST 差分（数十バイト）の伝播
- Git のような履歴管理を SDF レベルで実現
- 実装: `crates/network/src/vcs.rs`

### ALICE-Identity (DID)

- 分散型ユーザー ID。中央認証サーバー不要
- ライセンスキー（Ed25519）と同じ鍵ペアを DID に流用
- ユーザーの作品の帰属証明、フォーク履歴の署名に使用
- 実装: `crates/network/src/identity.rs`

### ネットワーク全体像

```
┌─ User A (General) ──────┐          ┌─ User B (Pro) ──────────┐
│                          │          │                          │
│  "猫の置物" → SDF 生成   │  P2P     │  ギャラリー閲覧          │
│       │                  │◄────────►│       │                  │
│       ▼                  │  Sync    │       ▼                  │
│  SDF → Cache (自動保持)  │          │  SDF → Cache (自動保持)  │
│       │                  │  CDN     │       │                  │
│       ▼                  │◄────────►│       ▼                  │
│  VCS: commit (Merkle)    │ Vivaldi  │  VCS: fork → diff 伝播   │
│       │                  │          │       │                  │
│  DID: 作品に署名          │          │  DID: フォークに署名      │
│                          │          │                          │
└──────────────────────────┘          └──────────────────────────┘
         │                                     │
         │        ┌─ User C (Free) ───────┐    │
         │        │                        │    │
         └───────►│  閲覧 → Cache にコピー  │◄───┘
           Sync   │  (このノードも CDN に)   │
                  │                        │
                  └────────────────────────┘

ノード数が増えるほど:
  - ネットワーク容量 ↑ (各ノードが CDN)
  - データ冗長性 ↑ (Cache が分散複製)
  - レイテンシ ↓ (Vivaldi で近距離優先)
  - 教師データ ↑ (General tier の SDF が蓄積)
  - サーバーコスト = 0 (常に)
```

### 起動シーケンス（ユーザーに見えない部分）

```
アプリ起動
  │
  ├── [表層] egui ウィンドウ表示、LLM ロード
  │
  └── [裏側] バックグラウンドスレッド
        ├── libp2p ノード起動 (ALICE-Sync)
        ├── Vivaldi 座標計算開始 (ALICE-CDN)
        ├── ローカル Cache インデックス構築 (ALICE-Cache)
        ├── DID 鍵ペア読み込み (ALICE-Identity)
        └── Merkle DAG ヘッド同期 (ALICE-VCS)
        
  → ユーザーには「アプリが起動した」としか見えない
  → 裏では P2P ネットワークに参加完了
```

---

## 8. ビルド・配布

### ビルドターゲット

| OS | ターゲット | 配布形式 |
|----|-----------|---------|
| macOS (Apple Silicon) | aarch64-apple-darwin | .dmg |
| macOS (Intel) | x86_64-apple-darwin | .dmg |
| Windows | x86_64-pc-windows-msvc | .msi |
| Linux | x86_64-unknown-linux-gnu | .AppImage |

### リリースプロファイル

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
```

### CI/CD

- GitHub Actions で各 OS 向けバイナリをビルド
- GitHub Releases でバイナリ配布
- 自動アップデート: egui 内にアップデート通知 UI

---

## 9. マイルストーン

### Phase 1: MVP（ローカル生成 + P2P 基盤）

表層（ユーザーに見える部分）:
- [ ] workspace + 4 クレート構成セットアップ (app / core / llm / network)
- [ ] egui/eframe アプリ骨格
- [ ] ALICE-View 埋め込み（SDF プレビュー）
- [ ] LLM 推論統合（llama-cpp-rs）
- [ ] Text → LOL → SDF → Mesh パイプライン
- [ ] SQLite DB + 生成履歴
- [ ] .3mf / .fbx エクスポート

裏側（ユーザーに見えない部分）:
- [ ] libp2p ノード起動（バックグラウンド）
- [ ] ALICE-Cache: ローカル SDF キャッシュストア
- [ ] ALICE-Identity: Ed25519 鍵ペア生成 + DID
- [ ] 初回起動時に P2P ネットワークへ静かに参加

### Phase 2: ティアシステム + データフライホイール

- [ ] Ed25519 ライセンス検証（DID 鍵ペアと統合）
- [ ] ティア制限（回数 / 公開義務 / DL 可否）
- [ ] ライセンスキー入力 UI
- [ ] Stripe 連携 Web ページ（キー発行）
- [ ] General tier: SDF 生成時に P2P ネットワークへ自動公開
- [ ] ALICE-Sync: gossipsub で SDF イベント伝播

### Phase 3: 分散ネットワーク完成

- [ ] ALICE-VCS: Merkle DAG でフォーク / リミックス差分管理
- [ ] ALICE-CDN: Vivaldi 座標計算 + 近距離ノード優先ルーティング
- [ ] ギャラリー: P2P ネットワーク上の公開 SDF をブラウズ
- [ ] キャッシュ伝播: 閲覧した SDF を自動キャッシュ → CDN ノード化

### Phase 4: 配布・ネットワーク拡大

- [ ] macOS / Windows / Linux バイナリ (.dmg / .msi / .AppImage)
- [ ] 自動アップデート
- [ ] モデルダウンローダー（初回起動時 LLM 取得）
- [ ] 多言語対応 (日/英)
- [ ] バイラルマーケティング（3D プリンター界隈）
- [ ] → 目標: 10 万インストール = 10 万ノード = サーバー代ゼロの分散インフラ完成
