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
    6. cargo の解決に出る sibling が siblings.toml に無い / siblings.toml に解決に出ない
       sibling がある (集合は manifest を目で追わず cargo の解決結果から求める:
       optional の path 依存は有効化されない限り要らないため)
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


def required_siblings(root: Path, siblings: Path) -> tuple[set[str], str | None]:
    """cargo の解決結果に出る sibling repo 名の集合 (失敗時は error 文字列)

    optional の path 依存は有効化されない限り解決に出ず、checkout も要らない
    (`cargo fmt --all` は path 依存の workspace の全 member まで読むが、CI の fmt は
    本 repo の package だけを見る)
    """
    proc = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--manifest-path", str(root / "Cargo.toml")],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if proc.returncode != 0:
        return set(), proc.stderr.strip() or f"cargo metadata exit {proc.returncode}"
    real_sib = Path(os.path.realpath(siblings))
    real_root = Path(os.path.realpath(root))
    need: set[str] = set()
    for pkg in json.loads(proc.stdout)["packages"]:
        if pkg.get("source") is not None:
            continue  # registry / git の依存
        mp = Path(os.path.realpath(pkg["manifest_path"]))
        if mp.parent == real_root or real_root in mp.parents:
            continue
        try:
            need.add(mp.relative_to(real_sib).parts[0])
        except ValueError:
            continue
    return need, None


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
        need, err = required_siblings(root, siblings)
        if err is not None:
            errors.append(f"cargo metadata が失敗 (sibling の欠落か版要求の不一致):\n{err}")
        elif not need:
            errors.append("cargo が読む sibling が 0 件 (検査が何も見ていない)")
        else:
            if need - set(pins):
                errors.append(f"cargo が読むのに {MANIFEST} に無い sibling: {', '.join(sorted(need - set(pins)))}")
            if set(pins) - need:
                errors.append(f"{MANIFEST} にあるが cargo が読まない sibling: {', '.join(sorted(set(pins) - need))}")
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

        need, err = required_siblings(t / "app", t)
        assert err is None, err
        assert need == {"ALICE-A", "ALICE-B"}, need  # optional 未有効の Z は解決に出ない

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
