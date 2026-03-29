# 3dvbgaran SaaS Launch Plan

## 現状
- [x] GitHub private repo (ext-sakamoro/3dvbgaran)
- [x] API Gateway (Rust/axum) — Railway デプロイ済み (3dvbgaran-production.up.railway.app)
- [x] Core Engine (Rust/axum) — コード完成、テスト4/4パス
- [x] Frontend (Next.js) — コード完成
- [x] DB Migrations — SQLファイル6本作成済み
- [x] Supabase プロジェクト作成済み (texttocad / erxsdbjpiiwzzeeelyie)

## Step 1: Supabase DB migrations 適用 ✅
- [x] `supabase link` でプロジェクト接続
- [x] migrations 6本を CLI (`supabase db push`) で適用
- [x] テーブル作成確認 (profiles, projects, plan_configs, api_usage, generations)
- [x] RLS ポリシー + トリガー動作確認
- 注意: `uuid_generate_v4()` → `gen_random_uuid()` に修正が必要だった

## Step 2: Railway 環境変数設定 (API Gateway)
- [ ] SUPABASE_URL=https://erxsdbjpiiwzzeeelyie.supabase.co
- [ ] SUPABASE_SERVICE_ROLE_KEY=(secret key)
- [ ] JWT_SECRET=(生成)
- [ ] CORE_ENGINE_URL=(Step 4で決定)

## Step 3: Core Engine デプロイ (Railway 別サービス)
- [ ] Railway に core-engine サービス追加
- [ ] Dockerfile.core-engine でビルド
- [ ] Private networking で API Gateway から接続
- [ ] 環境変数: LLM_ENDPOINT, OUTPUT_DIR

## Step 4: Frontend デプロイ
- [ ] Railway 別サービス or Vercel にデプロイ
- [ ] 環境変数: NEXT_PUBLIC_SUPABASE_URL, NEXT_PUBLIC_SUPABASE_ANON_KEY
- [ ] NEXT_PUBLIC_WORKER_URL=(API Gateway URL)
- [ ] STRIPE_SECRET_KEY, STRIPE_WEBHOOK_SECRET, NEXT_PUBLIC_STRIPE_PUBLISHABLE_KEY
- [ ] Public domain 発行

## Step 5: Stripe 設定
- [ ] Stripe Product 作成 (3dvbgaran Pro)
- [ ] Price 作成 ($19/mo)
- [ ] Webhook URL 設定 (Frontend URL/api/stripe/webhook)
- [ ] billing/page.tsx の priceId 更新

## Step 6: 動作確認
- [ ] ユーザー登録 (Supabase Auth)
- [ ] ログイン → Dashboard 表示
- [ ] Project 作成
- [ ] Generate ページ表示 (LLM未接続のためエラーは想定内)
- [ ] Billing ページ表示
- [ ] Settings ページ (API Key 生成)
- [ ] Stripe Checkout フロー (テストモード)

## Step 7: LLM 接続 (将来)
- [ ] LLM エンドポイント確保 (llama.cpp / Ollama / 外部API)
- [ ] Core Engine の LLM_ENDPOINT 環境変数設定
- [ ] alice-lol パイプライン統合 (.3mf バイナリ返却)
- [ ] E2E テスト (テキスト → .3mf 生成 → ダウンロード)
