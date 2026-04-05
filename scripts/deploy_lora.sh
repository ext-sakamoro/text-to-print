#!/bin/bash
# Paperspace → Mac Mini GGUF転送 & デプロイ
#
# 使い方:
#   bash scripts/deploy_lora.sh <paperspace_fqdn> <paperspace_token>

set -e

FQDN="${1:?Usage: deploy_lora.sh <fqdn> <token>}"
TOKEN="${2:?Usage: deploy_lora.sh <fqdn> <token>}"
MODEL_DIR="$HOME/.3dvbgaran/models"
MODEL_NAME="qwen2.5-7b-lol-lora-q4_k_m.gguf"

echo "=== Deploy LoRA GGUF ==="

# 1. Paperspace からダウンロード (Jupyter API経由)
echo "Downloading GGUF from Paperspace..."
# Jupyterのファイル取得API
curl -sL \
  "https://${FQDN}/files/3dvbgaran/lora_output_gguf/unsloth.Q4_K_M.gguf?token=${TOKEN}" \
  -o "${MODEL_DIR}/${MODEL_NAME}"

ls -lh "${MODEL_DIR}/${MODEL_NAME}"

# 2. llama-server を新モデルで再起動
echo ""
echo "Restarting llama-server with LoRA model..."
lsof -ti:8000 | xargs kill 2>/dev/null || true
sleep 1

eval "$(/opt/homebrew/bin/brew shellenv)"
llama-server \
  --model "${MODEL_DIR}/${MODEL_NAME}" \
  --port 8000 \
  --cont-batching \
  --parallel 8 \
  --ctx-size 8192 \
  --n-gpu-layers 99 \
  --flash-attn on &

sleep 10
curl -sf http://localhost:8000/health && echo " OK" || echo " FAILED"

echo ""
echo "=== Deploy Complete ==="
echo "Model: ${MODEL_DIR}/${MODEL_NAME}"
echo "Server: http://localhost:8000"
