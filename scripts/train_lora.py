#!/usr/bin/env python3
"""3dvbgaran QLoRA 学習スクリプト

Qwen2.5-7B-Instruct に LOL DSL 生成能力を LoRA で追加学習する。
Paperspace A6000 (48GB) / A100 (80GB) 対応。

使い方:
  pip install unsloth datasets
  python3 train_lora.py

出力:
  ./lora_output/         — LoRAアダプタ
  ./lora_output_merged/  — マージ済みモデル
  ./lora_output.gguf     — GGUF (Q4_K_M)
"""

import json
import os

# unsloth は import 順が重要 (torch より先)
from unsloth import FastLanguageModel
from datasets import Dataset
from trl import SFTTrainer, SFTConfig

# ─── 設定 ───

MODEL_NAME = "unsloth/Qwen2.5-7B-Instruct"
DATASET_PATH = os.environ.get("DATASET_PATH", "/notebooks/3dvbgaran/datasets/lol_train.jsonl")
OUTPUT_DIR = os.environ.get("OUTPUT_DIR", "./lora_output")
MAX_SEQ_LEN = 512
LORA_RANK = 32
LORA_ALPHA = 64
EPOCHS = 5
BATCH_SIZE = 4
GRAD_ACCUM = 2
LR = 2e-4

SYSTEM_PROMPT = (
    "You are a Text-to-CAD assistant. "
    "Convert the user's description into LOL DSL code for 3D printing. "
    "Output ONLY valid LOL DSL, no explanation."
)


def load_dataset():
    """JSONL → Dataset (ChatML形式に変換)"""
    examples = []
    with open(DATASET_PATH) as f:
        for line in f:
            d = json.loads(line)
            text = (
                f"<|im_start|>system\n{SYSTEM_PROMPT}<|im_end|>\n"
                f"<|im_start|>user\n{d['input']}<|im_end|>\n"
                f"<|im_start|>assistant\n{d['output']}<|im_end|>"
            )
            examples.append({"text": text})

    ds = Dataset.from_list(examples)
    print(f"Dataset: {len(ds)} examples")
    print(f"Sample:\n{ds[0]['text'][:300]}")
    return ds


def main():
    print("=== 3dvbgaran LoRA Training ===\n")

    # 1. モデルロード (4-bit量子化)
    print("Loading model...")
    model, tokenizer = FastLanguageModel.from_pretrained(
        model_name=MODEL_NAME,
        max_seq_length=MAX_SEQ_LEN,
        dtype=None,  # auto
        load_in_4bit=True,
    )

    # 2. LoRAアダプタ追加
    print(f"Adding LoRA adapter (rank={LORA_RANK})...")
    model = FastLanguageModel.get_peft_model(
        model,
        r=LORA_RANK,
        lora_alpha=LORA_ALPHA,
        lora_dropout=0,
        target_modules=[
            "q_proj", "k_proj", "v_proj", "o_proj",
            "gate_proj", "up_proj", "down_proj",
        ],
        bias="none",
        use_gradient_checkpointing="unsloth",
    )

    # 3. データセットロード
    ds = load_dataset()

    # 4. 学習設定
    trainer = SFTTrainer(
        model=model,
        tokenizer=tokenizer,
        train_dataset=ds,
        args=SFTConfig(
            output_dir=OUTPUT_DIR,
            per_device_train_batch_size=BATCH_SIZE,
            gradient_accumulation_steps=GRAD_ACCUM,
            num_train_epochs=EPOCHS,
            learning_rate=LR,
            lr_scheduler_type="cosine",
            warmup_ratio=0.1,
            bf16=True,
            logging_steps=5,
            save_steps=50,
            save_total_limit=3,
            seed=42,
            max_seq_length=MAX_SEQ_LEN,
            dataset_text_field="text",
            packing=True,
        ),
    )

    # 5. 学習実行
    print("\nStarting training...")
    stats = trainer.train()
    print(f"\nTraining complete: {stats}")

    # 6. LoRA保存
    print(f"\nSaving LoRA adapter to {OUTPUT_DIR}...")
    model.save_pretrained(OUTPUT_DIR)
    tokenizer.save_pretrained(OUTPUT_DIR)

    # 7. GGUF変換
    print("\nExporting to GGUF (Q4_K_M)...")
    model.save_pretrained_gguf(
        f"{OUTPUT_DIR}_gguf",
        tokenizer,
        quantization_method="q4_k_m",
    )
    print(f"\nGGUF saved to {OUTPUT_DIR}_gguf/")

    print("\n=== Done ===")
    print(f"  LoRA adapter: {OUTPUT_DIR}/")
    print(f"  GGUF: {OUTPUT_DIR}_gguf/")
    print(f"  Copy GGUF to Mac Mini: scp <paperspace>:{OUTPUT_DIR}_gguf/*.gguf ~/.3dvbgaran/models/")


if __name__ == "__main__":
    main()
