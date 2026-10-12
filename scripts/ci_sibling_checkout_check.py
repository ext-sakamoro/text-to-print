#!/usr/bin/env python3
"""ALICE-* sibling の固定 commit (`.github/siblings.toml`) と CI の checkout を検査する

text-to-print は ALICE-* を `../ALICE-Xxx` の path 依存で引く CI は sibling を
`.github/siblings.toml` に書いた commit で checkout する
(`.github/actions/alice-siblings`) 本 script は次を fail にする

  静的 (常に)
    1. siblings.toml の `[siblings]` が 0 件 / 値が 40 桁の SHA でない
    2. composite action が checkout する sibling の集合が siblings.toml と違う
    3. workflow の job が sibling を直接 checkout している (固定 commit を迂回する)
    4. composite action を使う job が 0 件 (検査が何も比較していない)
  cargo (`--no-cargo` で省略)
    5. `cargo metadata` が解決できない (sibling の欠落か版要求の不一致)
    6. cargo の解決に出る sibling が siblings.toml に無い / siblings.toml に cargo が
       参照しない sibling がある (集合は manifest を目で追わず cargo の出力から求める、
       詳細は required_siblings)
  `--verify-heads` (CI 用)
    7. checkout された各 sibling の HEAD が siblings.toml の commit と違う

usage: ci_sibling_checkout_check.py [--root <repo>] [--siblings <dir>]
                                    [--no-cargo] [--verify-heads] [--self-test]
  --root      text-to-print の root (既定: この script の親の親)
  --siblings  sibling repo が並ぶ directory (既定: root の親)
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

SIBLING_RE = re.compile(r"repository:\s*[\w.-]+/(ALICE-[\w-]+)")
ACTION_USE_RE = re.compile(r"uses:\s*\./(?:[\w.-]+/)?\.github/actions/alice-siblings\s*$", re.M)
JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
MANIFEST = Path(".github/siblings.toml")
ACTION = Path(".github/actions/alice-siblings/action.yml")


def norm(p: Path) -> Path:
    """symlink を解かずに正規化する"""
    return Path(os.path.normpath(os.path.abspath(p)))


def read_pins(root: Path) -> tuple[dict[str, str], list[str]]:
    errors: list[str] = []
    with (root / MANIFEST).open("rb") as f:
        pins = dict((tomllib.load(f).get("siblings") or {}).items())
    if not pins:
        errors.append(f"{MANIFEST} の [siblings] が 0 件")
    for name, sha in pins.items():
        if not isinstance(sha, str) or not SHA_RE.match(sha):
            errors.append(f"{MANIFEST}: {name} の値が 40 桁の commit SHA でない: {sha!r}")
    return pins, errors


def _metadata(manifest: Path, no_deps: bool) -> tuple[dict | None, str | None]:
    cmd = ["cargo", "metadata", "--format-version", "1", "--manifest-path", str(manifest)]
    if no_deps:
        cmd.append("--no-deps")
    proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
    if proc.returncode != 0:
        return None, f"{manifest}:\n{proc.stderr.strip() or f'cargo metadata exit {proc.returncode}'}"
    return json.loads(proc.stdout), None


def _sibling_of(path: Path, real_sib: Path, real_root: Path) -> str | None:
    p = Path(os.path.realpath(path))
    if p == real_root or real_root in p.parents:
        return None
    try:
        return p.relative_to(real_sib).parts[0]
    except ValueError:
        return None


def required_siblings(root: Path, siblings: Path) -> tuple[set[str], set[str], str | None]:
    """cargo が読む sibling を求める (失敗時は error 文字列)

    cargo は 2 通りの読み方をする
      - build / test: root の解決結果に出る path package だけを読む (optional の path
        依存は有効化されない限り読まない)
      - `cargo fmt --all`: path 依存の package ごとに `cargo metadata --no-deps` を掛ける
        その package が複数 member の workspace に属すると、全 member の path 依存
        (optional を含む) の manifest まで読む (ALICE-LOL の alice-lol-robot →
        ALICE-Kinematics で実測)
    返り値は (解決結果に出る sibling, 上の 2 通りで参照されうる sibling)
    前者は checkout から外せない集合、後者は「固定しても使われない」の判定に使う
    どちらの読み方でも、存在しない sibling は cargo 自身の失敗として返る
    """
    real_sib = Path(os.path.realpath(siblings))
    real_root = Path(os.path.realpath(root))
    meta, err = _metadata(root / "Cargo.toml", no_deps=False)
    if err is not None:
        return set(), set(), err
    resolved: set[str] = set()
    manifests: list[Path] = []
    for pkg in meta["packages"]:
        if pkg.get("source") is not None:
            continue  # registry / git の依存
        name = _sibling_of(Path(pkg["manifest_path"]), real_sib, real_root)
        if name is not None:
            resolved.add(name)
            manifests.append(Path(pkg["manifest_path"]))
    referenced = set(resolved)
    for manifest in manifests:
        nd, err = _metadata(manifest, no_deps=True)
        if err is not None:
            return resolved, referenced, err
        for pkg in nd["packages"]:
            for dep in pkg["dependencies"]:
                if dep.get("path"):
                    name = _sibling_of(Path(dep["path"]), real_sib, real_root)
                    if name is not None:
                        referenced.add(name)
    return resolved, referenced, None


def split_jobs(text: str) -> dict[str, str]:
    """workflow の `jobs:` 直下の job ごとに本文を切り出す"""
    lines = text.splitlines()
    try:
        start = next(i for i, l in enumerate(lines) if l.rstrip() == "jobs:")
    except StopIteration:
        return {}
    jobs: dict[str, list[str]] = {}
    cur: str | None = None
    for line in lines[start + 1 :]:
        if line and not line.startswith((" ", "#")):
            break
        m = JOB_RE.match(line)
        if m:
            cur = m.group(1)
            jobs[cur] = []
        elif cur is not None:
            jobs[cur].append(line)
    return {k: "\n".join(v) for k, v in jobs.items()}


def static_checks(root: Path, pins: dict[str, str]) -> tuple[list[str], int]:
    errors: list[str] = []
    action_set = set(SIBLING_RE.findall((root / ACTION).read_text(encoding="utf-8")))
    if action_set != set(pins):
        errors.append(
            f"{ACTION} の checkout と {MANIFEST} が不一致: "
            f"action のみ {sorted(action_set - set(pins))} / manifest のみ {sorted(set(pins) - action_set)}"
        )
    users = 0
    for wf in sorted((root / ".github/workflows").glob("*.yml")):
        for job, body in split_jobs(wf.read_text(encoding="utf-8")).items():
            direct = sorted(set(SIBLING_RE.findall(body)))
            if direct:
                errors.append(
                    f"{wf.name}:{job} が sibling を直接 checkout している ({', '.join(direct)}) "
                    f"— {ACTION.parent} を使う"
                )
            if ACTION_USE_RE.search(body):
                users += 1
    if users == 0:
        errors.append("composite action を使う job が 0 件 (検査が何も比較していない)")
    return errors, users


def verify_heads(siblings: Path, pins: dict[str, str]) -> list[str]:
    errors: list[str] = []
    for name, sha in sorted(pins.items()):
        proc = subprocess.run(
            ["git", "-C", str(siblings / name), "rev-parse", "HEAD"], capture_output=True, text=True
        )
        head = proc.stdout.strip()
        if proc.returncode != 0 or head != sha:
            errors.append(f"{name} の HEAD {head or '(取得失敗)'} が固定 commit {sha} と違う")
    return errors


def check(root: Path, siblings: Path, use_cargo: bool = True, heads: bool = False) -> int:
    root, siblings = norm(root), norm(siblings)
    pins, errors = read_pins(root)
    st_errors, users = static_checks(root, pins)
    errors += st_errors
    need: set[str] = set()
    if use_cargo:
        need, referenced, err = required_siblings(root, siblings)
        if err is not None:
            errors.append(f"cargo metadata が失敗 (sibling の欠落か版要求の不一致):\n{err}")
        elif not need:
            errors.append("cargo が読む sibling が 0 件 (検査が何も見ていない)")
        else:
            if need - set(pins):
                errors.append(f"cargo が読むのに {MANIFEST} に無い sibling: {', '.join(sorted(need - set(pins)))}")
            if set(pins) - referenced:
                errors.append(f"{MANIFEST} にあるが cargo が読まない sibling: {', '.join(sorted(set(pins) - referenced))}")
    if heads:
        errors += verify_heads(siblings, pins)

    print(f"pinned siblings ({len(pins)}): {', '.join(sorted(pins))}")
    if use_cargo:
        print(f"cargo-required siblings ({len(need)}): {', '.join(sorted(need))}")
    print(f"jobs using the pinned checkout: {users}")
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    return 1 if errors else 0


def self_test() -> int:
    """架空の crate 群で歯を確かめる"""
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)

        def w(rel: str, body: str) -> None:
            p = t / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(body, encoding="utf-8")

        def crate(rel: str, name: str, deps: str = "") -> None:
            w(f"{rel}/Cargo.toml", f'[package]\nname="{name}"\nversion="0.1.0"\nedition="2021"\n[dependencies]\n{deps}')
            w(f"{rel}/src/lib.rs", "")

        def git_init(rel: str) -> str:
            d = str(t / rel)
            for cmd in (
                ["git", "init", "-q"],
                ["git", "add", "-A"],
                ["git", "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "x"],
            ):
                subprocess.run(["git", "-C", d, *cmd[1:]] if cmd[0] == "git" else cmd, check=True)
            return subprocess.run(["git", "-C", d, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()

        w("app/Cargo.toml", '[workspace]\nmembers=["c"]\nresolver="2"\n')
        crate("app/c", "c", 'a={path="../../ALICE-A"}\n')
        crate("ALICE-A", "a", 'b={path="../ALICE-B"}\nz={path="../ALICE-Z",optional=true}\n')
        crate("ALICE-B", "b")
        crate("ALICE-Z", "z")
        sha_a, sha_b = git_init("ALICE-A"), git_init("ALICE-B")

        need, referenced, err = required_siblings(t / "app", t)
        assert err is None, err
        assert need == {"ALICE-A", "ALICE-B"}, need  # optional 未有効の Z は解決に出ない
        assert referenced == {"ALICE-A", "ALICE-B", "ALICE-Z"}, referenced

        good_pins = f'[siblings]\nALICE-A = "{sha_a}"\nALICE-B = "{sha_b}"\n'
        good_action = "steps:\n  - with:\n      repository: o/ALICE-A\n  - with:\n      repository: o/ALICE-B\n"
        good_wf = (
            "jobs:\n  build:\n    steps:\n      - uses: ./app/.github/actions/alice-siblings\n"
            "  lint:\n    steps:\n      - run: true\n"
        )

        def setup(pins: str = good_pins, action: str = good_action, wf: str = good_wf) -> None:
            w("app/.github/siblings.toml", pins)
            w("app/.github/actions/alice-siblings/action.yml", action)
            w("app/.github/workflows/ci.yml", wf)

        setup()
        assert check(t / "app", t, heads=True) == 0

        setup(pins=good_pins.replace(f'ALICE-B = "{sha_b}"\n', ""), action=good_action.replace("o/ALICE-B", "o/ALICE-A"))
        assert check(t / "app", t) == 1, "cargo が読むのに manifest に無い sibling を検出しない"

        setup(pins=good_pins + f'ALICE-Y = "{sha_a}"\n', action=good_action + "  - with:\n      repository: o/ALICE-Y\n")
        assert check(t / "app", t) == 1, "使われない manifest entry を検出しない"

        setup(action=good_action.replace("o/ALICE-B", "o/ALICE-X"))
        assert check(t / "app", t, use_cargo=False) == 1, "action と manifest の不一致を検出しない"

        setup(wf=good_wf + "  direct:\n    steps:\n      - with:\n          repository: o/ALICE-A\n")
        assert check(t / "app", t, use_cargo=False) == 1, "直接 checkout を検出しない"

        setup(wf="jobs:\n  lint:\n    steps:\n      - run: true\n")
        assert check(t / "app", t, use_cargo=False) == 1, "比較 0 件で fail しない"

        setup(pins=good_pins.replace(sha_b, sha_b[:7]))
        assert check(t / "app", t, use_cargo=False) == 1, "短縮 SHA を検出しない"

        setup(pins=good_pins.replace(sha_b, sha_a))
        assert check(t / "app", t, heads=True) == 1, "HEAD と固定 commit の不一致を検出しない"

        setup()
        (t / "ALICE-B/Cargo.toml").rename(t / "ALICE-B/Cargo.toml.off")
        assert check(t / "app", t) == 1, "存在しない path 依存を検出しない"
    print("self-test ok")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=norm(Path(__file__)).parent.parent)
    ap.add_argument("--siblings", type=Path, default=None)
    ap.add_argument("--no-cargo", action="store_true")
    ap.add_argument("--verify-heads", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    return check(a.root, a.siblings or norm(a.root).parent, not a.no_cargo, a.verify_heads)


if __name__ == "__main__":
    sys.exit(main())
