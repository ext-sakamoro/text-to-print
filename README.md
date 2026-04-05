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

### Qwen2.5 7B 段階的負荷テスト (ollama)

同一プロンプト (短い LOL DSL 生成) を N 並列で同時発行し、レイテンシとスループットを計測。

| 並列数 | wall時間 | min | p50 | p95 | max | avg | throughput |
|---|---|---|---|---|---|---|---|
| 1 | 0.84s | 0.81s | 0.81s | 0.81s | 0.81s | 0.81s | 1.2 req/s |
| 2 | 1.41s | 0.72s | 1.38s | 1.38s | 1.38s | 1.05s | 1.4 req/s |
| 3 | 2.08s | 0.73s | 1.38s | 2.05s | 2.05s | 1.38s | 1.4 req/s |
| 5 | 3.39s | 0.73s | 2.04s | 3.36s | 3.36s | 2.04s | 1.5 req/s |
| 8 | 5.39s | 0.75s | 3.37s | 5.35s | 5.35s | 3.05s | 1.5 req/s |
| 10 | 6.82s | 0.84s | 4.13s | 6.78s | 6.78s | 3.81s | 1.5 req/s |
| 15 | 10.71s | 0.78s | 5.70s | 10.67s | 10.67s | 5.70s | 1.4 req/s |
| 20 | 14.48s | 0.73s | 7.79s | 14.42s | 14.42s | 7.47s | 1.4 req/s |
| 30 | 22.11s | 0.89s | 11.71s | 21.23s | 22.05s | 11.36s | 1.4 req/s |
| 40 | 30.20s | 0.88s | 15.72s | 29.35s | 30.14s | 15.37s | 1.3 req/s |
| 50 | 38.11s | 1.03s | 19.74s | 36.53s | 37.99s | 19.40s | 1.3 req/s |

### Qwen2.5 7B 段階的負荷テスト (llama.cpp server, continuous batching, parallel=4)

ollama を llama.cpp server に置き換え、continuous batching + 4スロット並列で同一テストを実施。

| 並列数 | wall時間 | min | p50 | p95 | max | avg | throughput |
|---|---|---|---|---|---|---|---|
| 1 | 0.91s | 0.88s | 0.88s | 0.88s | 0.88s | 0.88s | 1.1 req/s |
| 2 | 1.24s | 1.20s | 1.21s | 1.21s | 1.21s | 1.20s | 1.6 req/s |
| 3 | 1.86s | 1.80s | 1.80s | 1.83s | 1.83s | 1.81s | 1.6 req/s |
| 5 | 2.48s | 1.76s | 1.76s | 2.45s | 2.45s | 1.93s | 2.0 req/s |
| 8 | 3.59s | 1.64s | 3.43s | 3.55s | 3.55s | 2.64s | 2.2 req/s |
| 10 | 4.68s | 1.59s | 3.46s | 4.64s | 4.64s | 2.98s | 2.1 req/s |
| 15 | 7.70s | 1.74s | 4.17s | 7.65s | 7.65s | 4.60s | 1.9 req/s |
| 20 | 9.95s | 1.59s | 5.73s | 9.89s | 9.89s | 5.67s | 2.0 req/s |
| 30 | 15.74s | 1.59s | 8.30s | 15.64s | 15.67s | 8.39s | 1.9 req/s |
| 40 | 18.90s | 1.67s | 10.81s | 18.81s | 18.81s | 10.19s | 2.1 req/s |
| 50 | 23.91s | 1.60s | 13.03s | 22.63s | 23.80s | 12.47s | 2.1 req/s |

### Qwen2.5 7B 段階的負荷テスト (llama.cpp server, continuous batching, parallel=8)

parallel=4 → 8 に増やした結果。KVキャッシュ 448MB (VRAM 25GB中 1.7%)。

| 並列数 | wall時間 | min | p50 | p95 | max | avg | throughput |
|---|---|---|---|---|---|---|---|
| 1 | 0.78s | 0.74s | 0.74s | 0.74s | 0.74s | 0.74s | 1.3 req/s |
| 2 | 1.33s | 1.26s | 1.29s | 1.29s | 1.29s | 1.28s | 1.5 req/s |
| 3 | 1.71s | 1.61s | 1.67s | 1.67s | 1.67s | 1.65s | 1.8 req/s |
| 5 | 2.32s | 2.14s | 2.22s | 2.28s | 2.28s | 2.22s | 2.2 req/s |
| 8 | 3.19s | 3.03s | 3.14s | 3.15s | 3.15s | 3.09s | **2.5 req/s** |
| 10 | 3.82s | 2.47s | 2.76s | 3.78s | 3.78s | 2.90s | **2.6 req/s** |
| 15 | 6.45s | 2.86s | 3.98s | 6.40s | 6.40s | 4.65s | 2.3 req/s |
| 20 | 8.37s | 2.87s | 6.52s | 8.32s | 8.32s | 5.46s | 2.4 req/s |
| 30 | 12.58s | 2.86s | 6.53s | 12.50s | 12.50s | 7.68s | 2.4 req/s |
| 40 | 18.50s | 3.00s | 10.89s | 18.32s | 18.41s | 10.81s | 2.2 req/s |
| 50 | 21.29s | 2.87s | 13.23s | 20.01s | 21.18s | 11.88s | **2.3 req/s** |

### 全構成比較

| 並列数 | ollama | llama.cpp p=4 | llama.cpp p=8 | p=8 vs ollama |
|---|---|---|---|---|
| 1 | 0.84s / 1.2 req/s | 0.91s / 1.1 req/s | 0.78s / 1.3 req/s | +8% |
| 5 | 3.39s / 1.5 req/s | 2.48s / 2.0 req/s | 2.32s / 2.2 req/s | **+47%** |
| 10 | 6.82s / 1.5 req/s | 4.68s / 2.1 req/s | 3.82s / 2.6 req/s | **+73%** |
| 20 | 14.48s / 1.4 req/s | 9.95s / 2.0 req/s | 8.37s / 2.4 req/s | **+71%** |
| 50 | 38.11s / 1.3 req/s | 23.91s / 2.1 req/s | 21.29s / 2.3 req/s | **+77%** |

### ボトルネック分析

**ollama (throughput ~1.4 req/s で頭打ち):**

1. **推論直列化 (`num_parallel=1`)** — 並列リクエストを内部キューで逐次処理。wall時間が N に比例（N=50 → 38s ≒ 0.81s × 47）
2. **Go HTTP → llama.cpp FFI のオーバーヘッド** — リクエストごとにGo/CGo境界を跨ぐ
3. **GPU演算は遊んでいる** — 直列処理のため、1リクエスト完了→次リクエスト開始の間にGPUがidle

**llama.cpp server parallel=4 (throughput ~2.1 req/s, ollamaから+50%):**

1. **continuous batching** — 4リクエストのトークンを1回のGPU演算にバッチ化。GPU idle時間を削減
2. **C++直接** — Go/Python層なし。HTTP → 推論が最短パス
3. **ボトルネック**: GPU演算能力。4スロットでもGPU使用率が飽和しきらない

**llama.cpp server parallel=8 (throughput ~2.5 req/s, ollamaから+77%):**

1. **8スロット同時バッチ** — GPU演算のバッチサイズ倍増。matmulの効率が向上
2. **N=8で最高効率 2.5 req/s** — 8スロットが全て埋まった状態が最適
3. **N>8でも2.2-2.4 req/s維持** — キュー待ちは増えるがスループットは安定
4. **KVキャッシュ 448MB** — VRAM 25GB中 1.7%。parallel=16以上も余裕あり
5. **最終ボトルネック**: M2 Pro GPU 19コアの演算能力上限。これ以上はハード増設が必要

### 同時ユーザー数の目安

| 体感 | ollama | llama.cpp p=4 | llama.cpp p=8 (推奨) |
|---|---|---|---|
| 快適 (< 1s) | 1人 | 1人 | **1〜2人** |
| 良好 (< 3s) | 3〜5人 | 5〜8人 | **8〜10人** |
| 許容 (< 5s) | 5〜8人 | 8〜10人 | **10〜15人** |
| 実用限界 (< 10s) | 10〜15人 | 15〜20人 | **20〜30人** |
| 劣化 (> 15s) | 20人以上 | 30人以上 | **40人以上** |

### 起動コマンド

```bash
# llama.cpp server (推奨構成)
llama-server \
  --model ~/.3dvbgaran/models/qwen2.5-7b-instruct-q4_k_m.gguf \
  --port 8000 \
  --cont-batching \
  --parallel 8 \
  --ctx-size 8192 \
  --n-gpu-layers 99 \
  --flash-attn on
```

## ライセンス

MIT
