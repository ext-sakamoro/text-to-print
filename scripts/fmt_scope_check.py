#!/usr/bin/env python3
"""`cargo fmt --check` を走らせ、本 repo の Rust file が全部検査されたことを確かめる

CI の fmt は本 repo の package だけを見る (`--all` は path 依存の sibling 全体を
1 回の rustfmt に渡し、Windows で引数長の上限に当たる) その代わりに、検査した
file の集合が「workspace member の directory にある追跡済み `.rs` 全部」と一致する
ことを assert する member が抜けたり file が検査から漏れたら fail、0 件も fail

usage: fmt_scope_check.py [--root <repo>] [--self-test]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

FORMATTING_RE = re.compile(r"^Formatting (.+\.rs)\s*$")


def norm(p: str | Path) -> str:
    return os.path.normcase(os.path.realpath(p))


def expected_files(root: Path) -> set[str]:
    """workspace member の directory にある、git が追跡している `.rs` file"""
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=root, capture_output=True, text=True, encoding="utf-8", check=True,
        ).stdout
    )
    member_dirs = [norm(Path(p["manifest_path"]).parent) for p in meta["packages"]]
    tracked = subprocess.run(
        ["git", "ls-files", "-z", "--", "*.rs"],
        cwd=root, capture_output=True, text=True, encoding="utf-8", check=True,
    ).stdout.split("\0")
    out: set[str] = set()
    for rel in filter(None, tracked):
        f = norm(root / rel)
        if any(f.startswith(d + os.sep) for d in member_dirs):
            out.add(f)
    return out


def run_fmt(root: Path) -> tuple[int, set[str], str]:
    proc = subprocess.run(
        ["cargo", "fmt", "--", "--check", "-v"],
        cwd=root, capture_output=True, text=True, encoding="utf-8",
    )
    checked = set()
    for line in (proc.stdout + "\n" + proc.stderr).splitlines():
        m = FORMATTING_RE.match(line.strip())
        if m:
            checked.add(norm(m.group(1)))
    return proc.returncode, checked, proc.stdout + proc.stderr


def check(root: Path) -> int:
    root = Path(norm(root))
    rc, checked, log = run_fmt(root)
    expected = expected_files(root)
    errors: list[str] = []
    if rc != 0:
        diff = "\n".join(l for l in log.splitlines() if not FORMATTING_RE.match(l.strip()))
        errors.append(f"cargo fmt --check が失敗 (exit {rc}):\n{diff}")
    if not expected:
        errors.append("workspace member の .rs が 0 件 (検査が何も見ていない)")
    if not checked:
        errors.append("rustfmt が検査した file が 0 件")
    missing = sorted(expected - checked)
    if missing:
        errors.append(f"検査から漏れた file {len(missing)} 件: {', '.join(os.path.relpath(m, root) for m in missing)}")
    print(f"fmt checked {len(checked)} file(s), repo workspace .rs {len(expected)} file(s)")
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    return 1 if errors else 0


def self_test() -> int:
    """架空の workspace で歯を確かめる (漏れ / 0 件 / 未整形)"""
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)

        def w(rel: str, body: str) -> None:
            p = t / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(body, encoding="utf-8")

        def git(*a: str) -> None:
            subprocess.run(["git", "-C", str(t), *a], check=True, capture_output=True)

        w("Cargo.toml", '[workspace]\nmembers=["a"]\nresolver="2"\n')
        w("a/Cargo.toml", '[package]\nname="a"\nversion="0.1.0"\nedition="2021"\n')
        w("a/src/lib.rs", "mod m;\n")
        w("a/src/m.rs", "pub fn f() {}\n")
        git("init", "-q")
        git("add", "-A")
        assert check(t) == 0

        w("a/src/orphan.rs", "pub fn g() {}\n")  # どの mod からも辿れない file は検査されない
        git("add", "-A")
        assert check(t) == 1, "検査から漏れた file を検出しない"
        (t / "a/src/orphan.rs").unlink()
        git("add", "-A")

        w("a/src/m.rs", "pub fn f(  ) {}\n")
        assert check(t) == 1, "未整形を検出しない"
    print("self-test ok")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    return self_test() if a.self_test else check(a.root)


if __name__ == "__main__":
    sys.exit(main())
