# 3dvbgaran SaaS Launch Plan

## アーキテクチャ

```
Railway (単一コンテナ)
  Rust API Gateway (port 8080) ──→ Next.js Frontend (port 3000, 内部)
  /health, /license               認証, UI, 課金
  /api/v1/* (認証+レートリミット)
  /api/v1/generate → Mac mini にプロキシ
  それ以外 → Next.js にプロキシ
         │
         │ Cloudflare Tunnel
         ▼
Mac mini (自宅)
  Core Engine (Rust/axum)
  LLM (Qwen3.5-9B) → LOL DSL → alice-lol → alice-sdf → .3mf
         │
         ▼
Supabase (texttocad / erxsdbjpiiwzzeeelyie)
  PostgreSQL + Auth + RLS
```

## 完了済み

- [x] Step 1: Supabase DB migrations 適用 (6本, `supabase db push`)
- [x] Step 2: Railway 環境変数設定 (SUPABASE_URL, SERVICE_ROLE_KEY, JWT_SECRET)
- [x] Step 3: Core Engine → API Gateway 統合 (単一バイナリ)
- [x] Step 4: Frontend デプロイ (Rust + Next.js 単一コンテナ)
- [x] Step 6: E2E テスト 17/17 パス (Playwright)
- [x] Admin ユーザー作成 (sakamoro@extoria.co.jp, Enterprise)

## Step 5: Stripe 設定 (後日)
- [ ] Stripe Product/Price 作成
- [ ] Webhook URL 設定
- [ ] billing/page.tsx priceId 更新

## Step 7: Mac mini パイプライン (マシン準備後)
- [ ] Mac mini に Core Engine デプロイ (alice-lol依存付き)
- [ ] LLM サーバー起動 (llama.cpp / Ollama)
- [ ] Cloudflare Tunnel 設定 (Mac mini ↔ Railway)
- [ ] Railway 環境変数 `CORE_ENGINE_URL` を Tunnel URL に設定
- [ ] E2E テスト (テキスト → .3mf 生成 → ダウンロード)

## Step 8: Mac mini 不要で先に進める機能
- [ ] メール認証フロー確認 (Supabase email confirm)
- [ ] 新規登録ページの改善 (パスワードバリデーション表示)
- [ ] `/dashboard` トップにサービス説明・ステータス表示
- [ ] LLM未接続時のユーザー向けエラーメッセージ改善
- [ ] favicon 追加
- [ ] OGP / メタデータ設定
- [ ] 404 ページ作成
- [ ] fbx エクスポート対応 (alice-sdf 側の機能追加が必要 → 後日)
