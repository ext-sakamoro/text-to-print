#!/usr/bin/env python3
"""LOL DSL 種データ抽出スクリプト

ALICE-LOL/ALICE-SDF/ALICE-Bamboo から全 LOL DSL パターンを抽出し、
LoRA学習用の種データ (JSONL) として出力する。

出力: datasets/lol_seeds.jsonl
"""

import json
import os
import re
import sys
from pathlib import Path

HOME = Path.home()
OUT_DIR = HOME / "3dvbgaran" / "datasets"
OUT_FILE = OUT_DIR / "lol_seeds.jsonl"

seeds = []
seen = set()


def add_seed(lol: str, source: str, category: str):
    lol = lol.strip()
    if not lol or lol in seen:
        return
    # 壊れたデータを除外
    if "foobar" in lol or "extra" in lol:
        return
    # 括弧の対応チェック
    if lol.count("(") != lol.count(")"):
        return
    seen.add(lol)
    seeds.append({"lol": lol, "source": source, "category": category})


def extract_parse_lol(filepath: Path, source: str):
    """parse_lol("...") からLOLを抽出"""
    if not filepath.exists():
        return
    text = filepath.read_text()
    for m in re.finditer(r'parse_lol\("([^"]+)"\)', text):
        add_seed(m.group(1), source, "test")
    # 複数行のparse_lol
    for m in re.finditer(r'parse_lol\(r#"(.*?)"#\)', text, re.DOTALL):
        add_seed(m.group(1).strip(), source, "test")


def extract_lol_macro(filepath: Path, source: str):
    """lol! { ... } マクロからLOLを抽出"""
    if not filepath.exists():
        return
    text = filepath.read_text()
    # lol! { ... } を抽出 (ネストした括弧に対応)
    pattern = r'lol!\s*\{'
    for m in re.finditer(pattern, text):
        start = m.end()
        depth = 1
        i = start
        while i < len(text) and depth > 0:
            if text[i] == '{':
                depth += 1
            elif text[i] == '}':
                depth -= 1
            i += 1
        if depth == 0:
            body = text[start:i - 1].strip()
            # field Name { ... } パターンを除去
            body = re.sub(r'^field\s+\w+\s*\{(.*)\}$', r'\1', body, flags=re.DOTALL).strip()
            # 空白正規化
            body = re.sub(r'\s+', ' ', body)
            if body and '(' in body:
                add_seed(body, source, "example")


def extract_sdfnode_patterns(filepath: Path, source: str):
    """SdfNode::xxx(...) パターンからLOL相当を抽出"""
    if not filepath.exists():
        return
    text = filepath.read_text()
    # SdfNode::sphere(1.0) → sphere(1.0)
    for m in re.finditer(r'SdfNode::(\w+)\(([^)]*)\)', text):
        func = m.group(1)
        args = m.group(2).strip()
        if func in ("new", "from"):
            continue
        # Vec3引数は除外
        if "Vec3" in args:
            continue
        lol = f"{func}({args})"
        add_seed(lol, source, "sdfnode")


def main():
    print("=== LOL DSL 種データ抽出 ===\n")

    # 1. runtime_parser テストケース
    parser_rs = HOME / "ALICE-LOL" / "alice-lol" / "src" / "runtime_parser.rs"
    extract_parse_lol(parser_rs, "runtime_parser")
    print(f"  runtime_parser: {len(seeds)} seeds")

    # 2. ALICE-LOL examples
    prev = len(seeds)
    examples_dir = HOME / "ALICE-LOL" / "alice-lol" / "examples"
    if examples_dir.exists():
        for f in sorted(examples_dir.glob("*.rs")):
            extract_lol_macro(f, f"example/{f.stem}")
            extract_parse_lol(f, f"example/{f.stem}")
    print(f"  ALICE-LOL examples: +{len(seeds) - prev} seeds")

    # 3. ALICE-LOL print_export (doctest)
    prev = len(seeds)
    print_export = HOME / "ALICE-LOL" / "alice-lol" / "src" / "print_export.rs"
    extract_parse_lol(print_export, "print_export")
    print(f"  print_export: +{len(seeds) - prev} seeds")

    # 4. ALICE-SDF tests
    prev = len(seeds)
    sdf_tests = HOME / "ALICE-SDF" / "tests"
    if sdf_tests.exists():
        for f in sorted(sdf_tests.rglob("*.rs")):
            extract_sdfnode_patterns(f, f"sdf_test/{f.stem}")
    print(f"  ALICE-SDF tests: +{len(seeds) - prev} seeds")

    # 5. ALICE-SDF examples
    prev = len(seeds)
    sdf_examples = HOME / "ALICE-SDF" / "examples"
    if sdf_examples.exists():
        for f in sorted(sdf_examples.rglob("*.rs")):
            extract_sdfnode_patterns(f, f"sdf_example/{f.stem}")
    print(f"  ALICE-SDF examples: +{len(seeds) - prev} seeds")

    # 6. ALICE-Bamboo
    prev = len(seeds)
    bamboo_dir = HOME / "ALICE-Bamboo"
    if bamboo_dir.exists():
        for f in sorted(bamboo_dir.rglob("*.rs")):
            extract_sdfnode_patterns(f, f"bamboo/{f.stem}")
            extract_lol_macro(f, f"bamboo/{f.stem}")
    print(f"  ALICE-Bamboo: +{len(seeds) - prev} seeds")

    # 出力
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUT_FILE, "w") as f:
        for seed in seeds:
            f.write(json.dumps(seed, ensure_ascii=False) + "\n")

    print(f"\n  Total: {len(seeds)} unique seeds")
    print(f"  Output: {OUT_FILE}")

    # カテゴリ別集計
    cats = {}
    for s in seeds:
        cats[s["category"]] = cats.get(s["category"], 0) + 1
    for k, v in sorted(cats.items()):
        print(f"    {k}: {v}")


if __name__ == "__main__":
    main()
