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
| LLM | Qwen3.5 9B (ollama, Metal GPU) |
| VRAM割当 | 25 GB |

### レイテンシ（逐次リクエスト）

| リクエスト | 応答時間 |
|---|---|
| #1 | 8.3s |
| #2 | 8.6s |
| #3 | 7.6s |
| #4 | 7.0s |
| #5 | 7.3s |
| **平均** | **~7.7s** |

### 並列リクエスト（3同時）

| 完了順 | 応答時間 |
|---|---|
| 1番目 | 7.4s |
| 2番目 | 12.7s |
| 3番目 | 19.5s |

### スループット

- 逐次処理: 約 **7〜8 req/分**
- ollamaデフォルト `num_parallel=1` のため並列リクエストは直列処理される
- 快適に利用できる同時ユーザー数: **1〜2人**

## ライセンス

MIT
