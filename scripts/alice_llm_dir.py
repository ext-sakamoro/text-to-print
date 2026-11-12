#!/usr/bin/env python3
"""The directory of the ALICE-LLM checkout cargo resolved for this workspace (from Cargo.lock).

The sidecar (alice-llm-server) is built from the same ALICE-LLM commit the app links: the git checkout
cargo made for the package `alice-llm` (`cargo fetch` first). This prints the root of that checkout,
the directory holding ALICE-LLM's own Cargo.toml / Cargo.lock / rust-toolchain.toml.

    python3 scripts/alice_llm_dir.py [--metadata FILE]   (default: runs `cargo metadata --frozen`)

Exit 0 with the directory on stdout; 1 when the workspace does not resolve `alice-llm` exactly once,
when it does not come from the ALICE-LLM git repository on branch main, or when the directory has no
Cargo.toml; 2 when the metadata could not be read.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

SOURCE_PREFIX = "git+https://github.com/ext-sakamoro/ALICE-LLM?branch=main#"


def checkout_dir(meta: dict) -> Path:
    """the ALICE-LLM checkout root; ValueError when the graph does not name exactly one git alice-llm"""
    pkgs = [p for p in meta["packages"] if p["name"] == "alice-llm"]
    if len(pkgs) != 1:
        raise ValueError(f"alice-llm resolved {len(pkgs)} time(s), want 1")
    p = pkgs[0]
    src = p.get("source") or ""
    if not src.startswith(SOURCE_PREFIX):
        raise ValueError(f"alice-llm comes from {src!r}, not ALICE-LLM on branch main")
    d = Path(p["manifest_path"]).parent
    # a workspace member: walk up to the checkout root (the directory with the lock)
    root = d
    while not (root / "Cargo.lock").is_file() and root.parent != root:
        root = root.parent
    if not (root / "Cargo.toml").is_file():
        raise ValueError(f"no Cargo.toml in {root}")
    return root


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--metadata", help="a `cargo metadata --format-version 1` document (default: run cargo)")
    a = ap.parse_args(argv)
    try:
        if a.metadata:
            text = Path(a.metadata).read_text(encoding="utf-8")
        else:
            text = subprocess.run(["cargo", "metadata", "--format-version", "1", "--frozen"],
                                  capture_output=True, text=True, encoding="utf-8", check=True).stdout
        meta = json.loads(text)
    except (OSError, subprocess.CalledProcessError, json.JSONDecodeError) as e:
        print(f"alice_llm_dir: the metadata could not be read: {e}", file=sys.stderr)
        return 2
    try:
        print(checkout_dir(meta))
    except ValueError as e:
        print(f"alice_llm_dir: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
