#!/usr/bin/env python3
"""peer-to-peer 層 (libp2p) が戻っていないことを確かめる

app は生成物を LAN に配っていた (libp2p の gossipsub + mDNS) その経路は取り除いた
共有は HTTPS の Gallery だけで、利用者の同意がある時だけ送る

見るもの:
- `Cargo.lock` の package に `libp2p` で始まる名前が無い (package 0 件は fail)
- git が追跡する `Cargo.toml` と `.rs` に `libp2p` の語が無い (検査 0 件は fail)

usage: no_p2p_guard.py [--root <repo>] [--self-test]
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

PACKAGE_NAME_RE = re.compile(r'^name = "([^"]+)"\s*$', re.MULTILINE)
TOKEN_RE = re.compile(r"libp2p", re.IGNORECASE)


def lock_violations(lock_text: str) -> tuple[int, list[str]]:
    """(package 数, libp2p で始まる package 名)"""
    names = PACKAGE_NAME_RE.findall(lock_text)
    return len(names), sorted({n for n in names if n.lower().startswith("libp2p")})


def source_violations(root: Path, files: list[str]) -> tuple[int, list[str]]:
    """(検査した file 数, libp2p の語を含む file:line)"""
    hits = []
    scanned = 0
    for rel in files:
        path = root / rel
        if not path.is_file():
            continue
        scanned += 1
        text = path.read_text(encoding="utf-8", errors="replace")
        for no, line in enumerate(text.splitlines(), 1):
            if TOKEN_RE.search(line):
                hits.append(f"{rel}:{no}")
    return scanned, hits


def tracked_sources(root: Path) -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z", "--", "*.rs", "*Cargo.toml"],
        check=True,
        capture_output=True,
    ).stdout.decode("utf-8")
    return [f for f in out.split("\0") if f]


def check(root: Path, files: list[str]) -> list[str]:
    errors = []
    lock = root / "Cargo.lock"
    if not lock.is_file():
        return [f"{lock} が無い"]
    packages, bad = lock_violations(lock.read_text(encoding="utf-8"))
    if packages == 0:
        errors.append("Cargo.lock に package が 1 つも無い (検査が空振り)")
    errors += [f"Cargo.lock に {n}" for n in bad]
    scanned, hits = source_violations(root, files)
    if scanned == 0:
        errors.append("検査した Cargo.toml / .rs が 0 件 (検査が空振り)")
    errors += [f"libp2p の語: {h}" for h in hits]
    print(f"no_p2p_guard: lock packages={packages} files={scanned} violations={len(errors)}")
    return errors


def self_test() -> None:
    clean_lock = '[[package]]\nname = "reqwest"\n\n[[package]]\nname = "serde"\n'
    dirty_lock = clean_lock + '\n[[package]]\nname = "libp2p-gossipsub"\n'
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "a.rs").write_text("fn main() {}\n", encoding="utf-8")
        (root / "Cargo.lock").write_text(clean_lock, encoding="utf-8")
        assert check(root, ["a.rs"]) == [], "clean tree must pass"

        (root / "Cargo.lock").write_text(dirty_lock, encoding="utf-8")
        assert check(root, ["a.rs"]) == ["Cargo.lock に libp2p-gossipsub"]

        (root / "Cargo.lock").write_text(clean_lock, encoding="utf-8")
        (root / "b.rs").write_text("use libp2p::Swarm;\n", encoding="utf-8")
        assert check(root, ["a.rs", "b.rs"]) == ["libp2p の語: b.rs:1"]

        (root / "Cargo.toml").write_text('[dependencies]\nLibP2P = "0.57"\n', encoding="utf-8")
        assert check(root, ["Cargo.toml"]) == ["libp2p の語: Cargo.toml:2"]

        assert any("0 件" in e for e in check(root, ["missing.rs"])), "0 files must fail"

        (root / "Cargo.lock").write_text("version = 4\n", encoding="utf-8")
        assert any("package が 1 つも無い" in e for e in check(root, ["a.rs"]))

        (root / "Cargo.lock").unlink()
        assert check(root, ["a.rs"]) == [f"{root / 'Cargo.lock'} が無い"]
    print("no_p2p_guard: self-test ok (7 cases)")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        self_test()
        return 0
    errors = check(args.root, tracked_sources(args.root))
    for e in errors:
        print(f"error: {e}", file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
