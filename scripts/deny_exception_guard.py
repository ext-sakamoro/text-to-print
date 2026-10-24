#!/usr/bin/env python3
"""The licence exceptions in deny.toml equal what the dependencies declare.

usage: deny_exception_guard.py [--metadata-json FILE] [--deny FILE] [--prefix alice-]

cargo-deny accepts a crate when its declared licence expression is satisfied by the global `allow`
list plus the crate's own `[[licenses.exceptions]]` entry. An exception wider than the declaration
lets a later licence change pass unseen, and an exception for a crate that is gone, or that the
global list already accepts, is left over. For every exception, and for every package whose name
starts with the prefix (the ALICE crates, taken at the commits `cargo metadata` resolves), this
fails when:
  looser   an exception allows a licence id the crate does not declare
  stale    an exception names a crate that is not a dependency, or one the global list accepts
  missing  a prefixed crate's declaration is not satisfied by the global list and its exception
  nothing compared (no prefixed package and no exception)
A licence change of a dependency (its pin moved) therefore names the exception to change in the
same commit. Deprecated SPDX ids are read as their current form (AGPL-3.0 is AGPL-3.0-only).
Without --metadata-json it runs `cargo metadata --format-version 1 --locked --all-features`.
Exit 0 / 1 (2 on a usage or input error).
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11: refuse rather than skip
    tomllib = None

DEPRECATED = {
    "AGPL-3.0": "AGPL-3.0-only", "GPL-2.0": "GPL-2.0-only", "GPL-3.0": "GPL-3.0-only",
    "LGPL-2.0": "LGPL-2.0-only", "LGPL-2.1": "LGPL-2.1-only", "LGPL-3.0": "LGPL-3.0-only",
    "GPL-2.0+": "GPL-2.0-or-later", "GPL-3.0+": "GPL-3.0-or-later", "LGPL-2.1+": "LGPL-2.1-or-later",
}


class ExprError(ValueError):
    pass


def norm(i: str) -> str:
    return DEPRECATED.get(i, i)


def tokens(expr: str) -> list[str]:
    # the old cargo form `MIT/Apache-2.0` is an OR
    out = re.findall(r"\(|\)|[^\s()]+", expr.replace("/", " OR "))
    if not out:
        raise ExprError(f"empty licence expression {expr!r}")
    return out


def parse(expr: str):
    """an expression tree: ("id", name) / ("or", a, b) / ("and", a, b); `WITH exc` binds to its id"""
    toks, pos = tokens(expr), 0

    def peek():
        return toks[pos] if pos < len(toks) else None

    def take():
        nonlocal pos
        t = peek()
        pos += 1
        return t

    def atom():
        t = take()
        if t == "(":
            node = orexpr()
            if take() != ")":
                raise ExprError(f"unbalanced parentheses in {expr!r}")
            return node
        if t is None or t in (")", "OR", "AND", "WITH"):
            raise ExprError(f"unexpected {t!r} in {expr!r}")
        name = norm(t)
        if peek() == "WITH":
            take()
            exc = take()
            if exc is None or exc in ("(", ")", "OR", "AND", "WITH"):
                raise ExprError(f"`WITH` without an exception in {expr!r}")
            name = f"{name} WITH {exc}"
        return ("id", name)

    def andexpr():
        node = atom()
        while peek() == "AND":
            take()
            node = ("and", node, atom())
        return node

    def orexpr():
        node = andexpr()
        while peek() == "OR":
            take()
            node = ("or", node, andexpr())
        return node

    tree = orexpr()
    if pos != len(toks):
        raise ExprError(f"trailing {toks[pos]!r} in {expr!r}")
    return tree


def terms(tree) -> set[str]:
    if tree[0] == "id":
        return {tree[1]}
    return terms(tree[1]) | terms(tree[2])


def satisfied(tree, allowed: set[str]) -> bool:
    if tree[0] == "id":
        return tree[1] in allowed
    a, b = satisfied(tree[1], allowed), satisfied(tree[2], allowed)
    return (a or b) if tree[0] == "or" else (a and b)


def check(packages: list[dict], deny: dict, prefix: str) -> tuple[list[str], int]:
    lic = deny.get("licenses") or {}
    allow = {norm(x) for x in lic.get("allow") or []}
    by_name: dict[str, list[dict]] = {}
    for p in packages:
        by_name.setdefault(p["name"], []).append(p)
    errs: list[str] = []
    compared = 0
    excepted: dict[str, set[str]] = {}
    for e in lic.get("exceptions") or []:
        name = e.get("name") or e.get("crate")
        ids = [norm(x) for x in e.get("allow") or []]
        compared += 1
        if not name or not ids:
            errs.append(f"exception {e!r}: no crate name or an empty allow list")
            continue
        pkgs = by_name.get(name)
        if not pkgs:
            errs.append(f"{name}: stale exception (not a dependency)")
            continue
        excepted.setdefault(name, set()).update(ids)
        for p in pkgs:
            decl = p.get("license")
            if not decl:
                errs.append(f"{name} {p['version']}: declares no SPDX licence (license-file only); "
                            f"an exception cannot be checked against it")
                continue
            try:
                tree = parse(decl)
            except ExprError as err:
                errs.append(f"{name} {p['version']}: {err}")
                continue
            extra = sorted(set(ids) - terms(tree))
            if extra:
                errs.append(f"{name} {p['version']}: looser exception: allows {extra}, "
                            f"the crate declares `{decl}`")
            if satisfied(tree, allow):
                errs.append(f"{name} {p['version']}: stale exception: the global allow list already "
                            f"accepts `{decl}`")
    for name, pkgs in sorted(by_name.items()):
        if not name.startswith(prefix):
            continue
        for p in pkgs:
            compared += 1
            decl = p.get("license")
            if not decl:
                errs.append(f"{name} {p['version']}: declares no SPDX licence (license-file only)")
                continue
            try:
                tree = parse(decl)
            except ExprError as err:
                errs.append(f"{name} {p['version']}: {err}")
                continue
            if not satisfied(tree, allow | excepted.get(name, set())):
                need = sorted(terms(tree) - allow)
                errs.append(f"{name} {p['version']}: missing exception: `{decl}` is not accepted by the "
                            f"global allow list; add an exception allowing the declared id it uses "
                            f"(one of {need})")
    return errs, compared


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("--metadata-json")
    ap.add_argument("--deny", default="deny.toml")
    ap.add_argument("--prefix", default="alice-")
    a = ap.parse_args(argv)
    if tomllib is None:
        print("deny_exception_guard: needs Python 3.11+ (tomllib)", file=sys.stderr)
        return 2
    try:
        deny = tomllib.loads(Path(a.deny).read_text(encoding="utf-8"))
        if a.metadata_json:
            meta = json.loads(Path(a.metadata_json).read_text(encoding="utf-8"))
        else:
            out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked", "--all-features"],
                                 check=True, capture_output=True, text=True).stdout
            meta = json.loads(out)
    except (OSError, ValueError, subprocess.CalledProcessError) as e:
        print(f"deny_exception_guard: cannot read the inputs: {e}", file=sys.stderr)
        return 2
    errs, compared = check(meta.get("packages") or [], deny, a.prefix)
    for e in errs:
        print(f"deny_exception_guard: {e}")
    print(f"deny_exception_guard: compared {compared} (exceptions + {a.prefix}* packages), {len(errs)} finding(s)")
    if compared == 0:
        print("deny_exception_guard: nothing compared")
        return 1
    return 1 if errs else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
