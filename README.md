# 3dvbgaran

3Dバーチャルガランドウ — ALICE SaaS ベースの3Dサービス。

## アーキテクチャ

```
┌──────────────────────────────────────┐
│  Railway                             │
│  ┌────────────────────────────────┐  │
│  │  API Gateway (Rust/axum)       │  │
│  │  - JWT/APIキー認証             │  │
│  │  - Stripe課金                  │  │
│  │  - レートリミット              │  │
│  └──────────┬─────────────────────┘  │
│  ┌──────────┴─────────────────────┐  │
│  │  Frontend (Next.js)            │  │
│  │  - Supabase Auth               │  │
│  │  - Stripe Billing              │  │
│  └────────────────────────────────┘  │
│  ┌────────────────────────────────┐  │
│  │  Core Engine (Rust)            │  │
│  └────────────────────────────────┘  │
└──────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────┐
│  Supabase                            │
│  - PostgreSQL                        │
│  - Auth (Row Level Security)         │
│  - Storage                           │
└──────────────────────────────────────┘
```

## 技術スタック

| レイヤー | 技術 |
|---------|------|
| API | Rust / axum |
| フロントエンド | Next.js / React / Supabase / Stripe |
| DB | Supabase PostgreSQL |
| 認証 | Supabase Auth (RLS) |
| 課金 | Stripe |
| ホスティング | Railway |

## セットアップ

```bash
# API Gateway
cd services/api-gateway
cargo build --release

# Frontend
cd frontend
npm install && npm run build
```

## ベンチマーク

### 環境

| 項目 | スペック |
|------|---------|
| マシン | Mac Mini |
| チップ | Apple M2 Pro |
| メモリ | 32 GB |
| LLM推論 | ollama (Metal GPU, VRAM 25GB) |

### モデル比較 (LOL DSL生成精度)

テストプロンプト: Cube / Phone Stand / Vase の3パターン

| モデル | サイズ | Cube | Phone Stand | Vase | 平均応答 | 評価 |
|---|---|---|---|---|---|---|
| Qwen2.5 1.5B | 986MB | OK | 構文不正確 | 存在しない属性 | ~1s | 不可 |
| Qwen2.5 3B | 1.9GB | OK | 構造理解 | 構文不正確 | ~1.2s | 実用下限 |
| **Qwen2.5 7B** | **4.7GB** | **OK** | **構造理解** | **subtractで薄壁** | **~2.6s** | **推奨** |
| Qwen3.5 9B | 6.6GB | OK | thinking消化で未完了 | thinking消化で未完了 | ~45s | 不適 |

**結論**: Qwen2.5 7B が精度・速度のバランスで最適。Qwen3.5 9Bはthinkingモードで時間を浪費し、複雑なプロンプトでmax_tokensを超過する。

### Qwen2.5 7B レイテンシ詳細

| リクエスト | 応答時間 |
|---|---|
| Cube (単純) | 1.9s |
| Phone Stand (中程度) | 2.8s |
| Vase (複雑) | 3.0s |
| **平均** | **~2.6s** |

### スループット (ollama)

- 逐次処理: 約 **20〜25 req/分** (Qwen2.5 7B)
- ollamaデフォルト `num_parallel=1` のため並列リクエストは直列処理
- 快適に利用できる同時ユーザー数: **3〜5人**

## ライセンス

MIT
