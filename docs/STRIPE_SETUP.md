# Stripe セットアップ手順 (Phase S1)

text-to-print の Paid tier (subscription) を Cloudflare Workers 経由で
Stripe に接続する手順 Test mode で完結する scaffold なので、実 Live 課金
開始は Phase S3 で切替 (env 変数 1 個の差替)

## アーキテクチャ概要

```
[app]
   └─ "Upgrade" click
        → POST https://text-to-print.alicelaw.net/stripe/checkout-session
           body: { price_id, user_email }
        → Response: { url, session_id }
   └─ browser で Stripe Checkout URL open
                    ↓ user が Stripe で支払い
[Stripe webhook]
   → POST https://text-to-print.alicelaw.net/stripe/webhook
      (Stripe-Signature ヘッダー付、HMAC-SHA256 検証)
   → subscribers table upsert + Ed25519 でライセンス発行
   → licenses table insert + Resend 経由 email 送信
                    ↓
[user] email 受信 → app Settings > Enter License Key に貼付
   → LicenseVerifier で Ed25519 検証 → Tier::Pro 有効化 → DB persist
```

## 前提

- Stripe アカウント (Test mode で OK、Live 切替は Phase S3)
- Cloudflare アカウント + `wrangler login` 済
- Stripe CLI (本 repo は 1.27.0 で検証済、`stripe --version` で確認)
- Resend アカウント (optional、無いなら email は log 出力のみ、license 自体は D1 に保存されるので operator 手動配信可能)

## Step 0: local dev 前準備

```bash
# Stripe CLI 認証 (test mode)
stripe login

# Cloudflare 認証
cd ~/text-to-print/crates/worker
wrangler login
```

## Step 1: D1 database 作成 + schema migration

```bash
cd ~/text-to-print/crates/worker

# 初回のみ D1 作成
wrangler d1 create text-to-print-shares
# → 出力の database_id を wrangler.toml の [[d1_databases]] database_id にコピー

# schema 適用 (dev は --local、production は --remote)
wrangler d1 execute text-to-print-shares --file=migrations/0001_init.sql --local
wrangler d1 execute text-to-print-shares --file=migrations/0002_stripe.sql --local
# production 反映は --remote に差替
```

## Step 2: Stripe Product / Price 作成

Pro tier は Monthly / Yearly の 2 price、Enterprise は Stripe 外 (manual 問合せ)
Free tier は Stripe 課金なし (アプリ default 状態、LoRA share 経路のみ)

```bash
# Product 作成
stripe products create \
  --name "text-to-print Pro" \
  --description "個人利用向け Pro プラン (無制限生成 + オフライン推論)"
# → 返る id を PROD_ID として控える (prod_XXXXXXXXXXXXXX)

# Monthly price 作成 (¥3,000/月)
stripe prices create \
  --product=$PROD_ID \
  --unit-amount=3000 \
  --currency=jpy \
  --recurring[interval]=month \
  --nickname "Pro Monthly"
# → 返る id を PRICE_ID_PRO_MONTHLY として控える (price_XXXXXXXXXXXXXX)

# Yearly price 作成 (¥30,000/年、月額の 10 ヶ月分)
stripe prices create \
  --product=$PROD_ID \
  --unit-amount=30000 \
  --currency=jpy \
  --recurring[interval]=year \
  --nickname "Pro Yearly"
# → 返る id を PRICE_ID_PRO_YEARLY として控える
```

## Step 3: Ed25519 signing key 生成

```bash
cd ~/text-to-print
cargo run --release --package text-to-print-core --bin gen-license-key
# stdout に SECRET_KEY=<64 char hex>、stderr に公開鍵 (Rust src 形式) が出力される
```

出力例:
```
=== License Key Pair Generated ===
Store the secret key securely. Never commit it to git.

SECRET_KEY=abcd...ef01   # ← これを LICENSE_SIGNING_KEY_HEX に登録

const LICENSE_PUBLIC_KEY: [u8; 32] = [0xab, 0xcd, ...];  # ← app 側の LicenseVerifier に埋め込む
```

**運用注意**: SECRET_KEY を紛失すると全 subscriber の license 再発行が必要 password manager or `wrangler secret put` に即登録 + local バックアップ

## Step 4: Wrangler Secrets 登録

```bash
cd ~/text-to-print/crates/worker

# Stripe test mode secret (Live 切替は sk_live_ に差替)
echo -n "sk_test_YOUR_STRIPE_SECRET" | wrangler secret put STRIPE_SECRET_KEY

# 後述 Step 6 の stripe listen が出力する whsec_ を後で put
# echo -n "whsec_YOUR_WEBHOOK_SECRET" | wrangler secret put STRIPE_WEBHOOK_SECRET

# Step 3 の SECRET_KEY を put
echo -n "abcd...ef01" | wrangler secret put LICENSE_SIGNING_KEY_HEX

# Step 2 の Price ID を put (yearly は optional)
echo -n "price_XXXXXXXXXXXXXX" | wrangler secret put PRICE_ID_PRO_MONTHLY
echo -n "price_YYYYYYYYYYYYYY" | wrangler secret put PRICE_ID_PRO_YEARLY

# Email delivery (optional; unset なら log only)
echo -n "re_YOUR_RESEND_API_KEY" | wrangler secret put RESEND_API_KEY
```

登録済 secret 確認:
```bash
wrangler secret list
```

## Step 5: local wrangler dev 起動

```bash
cd ~/text-to-print/crates/worker
wrangler dev --local
# → http://localhost:8787 で listen
```

## Step 6: local webhook forwarding (dev 中)

別 terminal で:
```bash
stripe listen --forward-to http://localhost:8787/stripe/webhook
# → 起動時に出力される "webhook signing secret" (whsec_XXXX) を控える
```

その whsec_ を Step 4 の Wrangler secret に put:
```bash
echo -n "whsec_XXXX" | wrangler secret put STRIPE_WEBHOOK_SECRET
```

## Step 7: e2e 動作確認

### 7-1: checkout session 作成 API 呼出し

```bash
curl -X POST http://localhost:8787/stripe/checkout-session \
  -H "Content-Type: application/json" \
  -d '{"price_id":"price_XXXXXXXXXXXXXX","user_email":"you@example.com"}'
# → { "url": "https://checkout.stripe.com/c/pay/cs_test_...", "session_id": "cs_test_..." }
```

### 7-2: Checkout URL を browser で開く → Stripe test card で支払い

Test card:
- 番号: `4242 4242 4242 4242`
- 有効期限: 任意の未来
- CVC: 任意 3 桁

### 7-3: webhook 受信確認

`stripe listen` を実行している terminal に:
```
2026-08-07 22:00:00   --> checkout.session.completed [evt_...]
2026-08-07 22:00:00   <-- [200] POST http://localhost:8787/stripe/webhook
```

wrangler dev の console に:
```
[email stub] would send license to you@example.com tier=Pro key_len=XXX
```
(RESEND_API_KEY 未設定時) または
```
[email] license delivered to you@example.com tier=Pro
```
(RESEND_API_KEY 設定時)

### 7-4: D1 にレコード確認

```bash
wrangler d1 execute text-to-print-shares --local \
  --command "SELECT * FROM subscribers WHERE email='you@example.com'"
wrangler d1 execute text-to-print-shares --local \
  --command "SELECT id, tier, issued_at, expires_at FROM licenses ORDER BY issued_at DESC LIMIT 1"
```

### 7-5: 手動 event trigger (Stripe 支払い操作なしで webhook path テスト)

```bash
stripe trigger checkout.session.completed
stripe trigger customer.subscription.updated
stripe trigger customer.subscription.deleted
```

## Step 8: Production deploy (Phase S1 終盤 or S3)

```bash
cd ~/text-to-print/crates/worker
# schema を remote D1 に適用 (初回のみ)
wrangler d1 execute text-to-print-shares --file=migrations/0001_init.sql --remote
wrangler d1 execute text-to-print-shares --file=migrations/0002_stripe.sql --remote

# deploy
wrangler deploy
# → https://text-to-print.<workers.dev subdomain>/ にデプロイされる
# custom domain (text-to-print.alicelaw.net) は Cloudflare dashboard で route 設定
```

Production webhook endpoint URL (`https://text-to-print.alicelaw.net/stripe/webhook`) を
Stripe Dashboard > Developers > Webhooks に登録し、そこで生成される新しい
`whsec_` を production 用 secret として put:
```bash
echo -n "whsec_LIVE_XXX" | wrangler secret put STRIPE_WEBHOOK_SECRET
```

## トラブルシューティング

### `signature verify: signature mismatch`

Stripe Dashboard で発行された webhook signing secret と Wrangler secret が
不一致 `wrangler secret list` で確認、正しい whsec_ を put し直す

### `unknown price id price_XXX — add to PRICE_ID_* env vars`

新しい price を作成した場合、`PRICE_ID_PRO_MONTHLY` / `PRICE_ID_PRO_YEARLY`
の Wrangler secret を再登録 (別 tier を追加するなら `PRICE_ID_<TIER>_<CYCLE>`
の 命名で env var 追加 + `stripe_webhook.rs::tier_for_price_id` に mapping 追加)

### email が届かない

1. `RESEND_API_KEY` 未設定 → wrangler dev console に `[email stub]` log 出力される (期待動作)
2. `RESEND_API_KEY` 設定済だが失敗 → `[email] resend api NNN: ...` log で status code 確認
3. Resend dashboard で API key の domain 認証 (`text-to-print.alicelaw.net` DNS SPF/DKIM/DMARC) 済か確認

### license key が verify 失敗

- Ed25519 公開鍵の app 埋込値と worker 側 `LICENSE_SIGNING_KEY_HEX` の pair 不一致
  → Step 3 で新規生成 → app 側公開鍵 hard code 差替 (Phase S2 で実装)
- License の `expires_at` 経過 → webhook 側で `customer.subscription.updated` が正常に届いているか確認

## 関連

- **`docs/RELEASE.md`** — release automation 全般
- **`ROADMAP.md`** — P2-1 (Stripe 実装) の phase 分割 (S1 / S2 / S3)
- **`crates/worker/src/stripe_webhook.rs`** — webhook 処理本体
- **`crates/worker/src/license_issue.rs`** — Ed25519 signer (wire format contract は `crates/core/src/license.rs` と共有)
