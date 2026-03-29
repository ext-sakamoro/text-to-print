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

## ライセンス

MIT
