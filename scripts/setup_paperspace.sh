#!/bin/bash
# Paperspace セットアップ & 学習実行スクリプト
#
# 使い方 (Paperspace Jupyter ターミナルで実行):
#   curl -sL https://raw.githubusercontent.com/ext-sakamoro/3dvbgaran/main/scripts/setup_paperspace.sh | bash
#
# または手動:
#   git clone ... && cd 3dvbgaran && bash scripts/setup_paperspace.sh

set -e

echo "=== 3dvbgaran LoRA Setup (Paperspace) ==="

# 1. リポジトリ取得
if [ ! -d /notebooks/3dvbgaran ]; then
    echo "Cloning 3dvbgaran..."
    cd /notebooks
    git clone https://github.com/ext-sakamoro/3dvbgaran.git
fi
cd /notebooks/3dvbgaran

# 2. Python依存
echo "Installing dependencies..."
pip install -q "unsloth[colab-new] @ git+https://github.com/unslothai/unsloth.git"
pip install -q datasets trl

# 3. GPU確認
echo ""
echo "=== GPU Info ==="
nvidia-smi --query-gpu=name,memory.total,driver_version --format=csv,noheader
echo ""

# 4. データセット確認
echo "=== Dataset ==="
wc -l datasets/lol_train.jsonl
echo ""

# 5. 学習実行
echo "=== Starting Training ==="
DATASET_PATH=/notebooks/3dvbgaran/datasets/lol_train.jsonl \
OUTPUT_DIR=/notebooks/3dvbgaran/lora_output \
python3 scripts/train_lora.py

echo ""
echo "=== Training Complete ==="
echo "GGUF files:"
ls -lh /notebooks/3dvbgaran/lora_output_gguf/*.gguf 2>/dev/null || echo "  (check lora_output_gguf/)"
