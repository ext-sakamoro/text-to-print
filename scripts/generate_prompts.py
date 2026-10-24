#!/usr/bin/env python3
"""LOL DSL 種データ → LoRA学習用データセット生成

1. 各LOL DSLに対して自然言語プロンプトを生成 (llama.cpp server使用)
2. パラメータバリエーションで水増し
3. 出力: datasets/lol_train.jsonl (instruction-tuning形式)
"""

import json
import random
import re
import sys
import time
import urllib.request
from pathlib import Path

HOME = Path.home()
SEEDS_FILE = HOME / "text-to-print" / "datasets" / "lol_seeds.jsonl"
OUT_FILE = HOME / "text-to-print" / "datasets" / "lol_train.jsonl"
LLM_URL = "http://localhost:8000/v1/chat/completions"

SYSTEM_PROMPT = """You are a dataset generator. Given a LOL DSL code snippet for 3D modeling, generate a natural language prompt that a user would type to create this 3D object.

Rules:
- Output ONLY the user prompt, nothing else
- Be natural and varied (sometimes brief, sometimes detailed)
- Include dimensions in mm when the LOL has specific sizes
- Don't mention LOL DSL or function names
- Write in English"""


def call_llm(lol_code: str) -> str:
    """LOL DSLから自然言語プロンプトを生成"""
    body = json.dumps({
        "model": "qwen2.5-7b",
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": f"Generate a user prompt for this 3D shape:\n{lol_code}"}
        ],
        "temperature": 0.7,
        "max_tokens": 128
    }).encode()

    req = urllib.request.Request(LLM_URL, data=body, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.loads(resp.read())
            return data["choices"][0]["message"]["content"].strip().strip('"')
    except Exception as e:
        return ""


def make_training_example(prompt: str, lol: str) -> dict:
    """instruction-tuning形式のデータを生成"""
    return {
        "instruction": "Convert the user's description into LOL DSL code for 3D printing.",
        "input": prompt,
        "output": lol
    }


def vary_params(lol: str) -> list:
    """パラメータを変えてバリエーション生成"""
    variants = []
    # 数値を検出して変動
    numbers = re.findall(r'(\d+\.\d+)', lol)
    if not numbers:
        return variants

    for _ in range(2):
        new_lol = lol
        for num_str in set(numbers):
            num = float(num_str)
            # 0.5x ~ 2.0x でランダム変動
            factor = random.uniform(0.5, 2.0)
            new_val = round(num * factor, 2)
            if new_val > 0.01:
                new_lol = new_lol.replace(num_str, str(new_val), 1)
        if new_lol != lol:
            variants.append(new_lol)

    return variants


def main():
    print("=== LOL LoRA Dataset Generator ===\n")

    # 種データ読み込み
    seeds = []
    with open(SEEDS_FILE) as f:
        for line in f:
            seed = json.loads(line)
            # テンプレート変数を含むものは除外
            if '{' in seed["lol"]:
                continue
            # コメント付きは除外
            if '//' in seed["lol"]:
                continue
            seeds.append(seed)

    print(f"  Valid seeds: {len(seeds)}")

    examples = []
    failed = 0

    # 1. 各種データに自然言語プロンプトを生成
    print(f"\n--- Phase 1: Generating prompts via LLM ---")
    for i, seed in enumerate(seeds):
        lol = seed["lol"]
        prompt = call_llm(lol)
        if prompt:
            examples.append(make_training_example(prompt, lol))
            # パラメータバリエーション
            for variant in vary_params(lol):
                variant_prompt = call_llm(variant)
                if variant_prompt:
                    examples.append(make_training_example(variant_prompt, variant))
        else:
            failed += 1

        if (i + 1) % 10 == 0:
            print(f"  {i + 1}/{len(seeds)} processed, {len(examples)} examples, {failed} failed")

    # 2. 手動で確実なペアを追加 (LLMに頼らない基本形)
    manual_pairs = [
        ("A sphere with 10mm radius", "sphere(1.0)"),
        ("Create a cube with 20mm sides", "box3d(1.0, 1.0, 1.0)"),
        ("A 30mm cube", "box3d(1.5, 1.5, 1.5)"),
        ("Make a cylinder, 10mm radius and 20mm tall", "cylinder(1.0, 1.0)"),
        ("A donut shape", "torus(1.0, 0.3)"),
        ("A cone 10mm radius 20mm height", "cone(1.0, 2.0)"),
        ("A pill shape", "capsule(0.3, 1.0)"),
        ("An egg", "egg(1.0, 0.3)"),
        ("A heart shape", "heart(1.0)"),
        ("A diamond", "diamond(0.8, 1.0)"),
        ("A hexagonal prism", "hex_prism(0.5, 1.0)"),
        ("A tube with 10mm outer and 8mm inner radius", "tube(1.0, 0.8, 1.0)"),
        ("A star shape with 5 points", "star_polygon(1.0, 5.0, 0.4, 0.3)"),
        ("A cross shape", "cross_shape(1.0, 0.3, 0.05, 0.3)"),
        ("A box with rounded edges", "rounded_box(1.0, 1.0, 1.0, 0.1)"),
        ("A wireframe box", "box_frame(1.0, 1.0, 1.0, 0.1)"),
        ("A hollow sphere", "onion(0.1, sphere(1.0))"),
        ("A sphere with a hole through the middle", "subtract(sphere(1.0), cylinder(0.3, 2.0))"),
        ("Two spheres merged together", "union(sphere(1.0), translate(1.5, 0.0, 0.0, sphere(0.8)))"),
        ("A smooth blend of a sphere and a cube", "smooth_union(0.3, sphere(1.0), box3d(0.8, 0.8, 0.8))"),
        ("A sphere with a cube cut out of it", "subtract(sphere(1.0), box3d(0.6, 0.6, 0.6))"),
        ("A twisted box", "twist(0.5, box3d(1.0, 2.0, 1.0))"),
        ("A tapered cylinder", "taper(0.3, cylinder(1.0, 2.0))"),
        ("Six spheres arranged in a circle", "polar_repeat(6, translate(2.0, 0.0, 0.0, sphere(0.3)))"),
        ("A sphere mirrored on the X axis", "mirror(1.0, 0.0, 0.0, translate(1.0, 0.0, 0.0, sphere(0.3)))"),
        ("A vase with thin walls", "onion(0.1, smooth_union(1.0, sphere(1.5), translate(0.0, 2.0, 0.0, cylinder(0.75, 2.0))))"),
        ("A phone stand with a cable hole", "subtract(smooth_union(0.3, box3d(4.0, 3.0, 0.25), rotate(75.0, 0.0, 0.0, box3d(4.0, 2.0, 0.25))), translate(0.0, -2.5, 0.0, cylinder(0.5, 0.5)))"),
        ("A pen holder with 3 holes", "subtract(subtract(subtract(cylinder(1.5, 2.0), translate(-0.6, 0.0, 0.0, cylinder(0.4, 2.5))), translate(0.6, 0.0, 0.0, cylinder(0.4, 2.5))), translate(0.0, 0.0, 0.0, cylinder(0.4, 2.5)))"),
        ("A coaster with a ring pattern", "subtract(cylinder(2.5, 0.125), translate(0.0, 0.125, 0.0, torus(2.0, 0.06)))"),
        ("A crown", "round(0.02, smooth_union(0.04, onion(0.04, subtract(cylinder(0.9, 0.12), cylinder(0.78, 0.20))), polar_repeat(5, translate(0.85, 0.0, 0.0, rotate(0.0, 0.0, 90.0, rounded_cone(0.10, 0.03, 0.35))))))"),
    ]

    for prompt, lol in manual_pairs:
        examples.append(make_training_example(prompt, lol))

    # 出力
    OUT_FILE.parent.mkdir(parents=True, exist_ok=True)
    with open(OUT_FILE, "w") as f:
        for ex in examples:
            f.write(json.dumps(ex, ensure_ascii=False) + "\n")

    print(f"\n=== Done ===")
    print(f"  Total examples: {len(examples)}")
    print(f"  Manual pairs: {len(manual_pairs)}")
    print(f"  LLM-generated: {len(examples) - len(manual_pairs)}")
    print(f"  Failed LLM calls: {failed}")
    print(f"  Output: {OUT_FILE}")


if __name__ == "__main__":
    main()
