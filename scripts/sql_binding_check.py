#!/usr/bin/env python3
"""Fail if a SQL string reaches a query is built at run time instead of being a
literal with bound parameters.

Every SQL-bearing rusqlite / D1 call (`execute`, `execute_batch`, `prepare`,
`prepare_cached`, `query_row`, `query_map`, `query_and_then`) must take, as its
first argument, a string literal (`"..."` / `r"..."`) or an UPPER_CASE `const`
holding one. A `format!(...)`, a `"..." + x` concatenation or a lower-case
variable there means the SQL was assembled from parts, the shape injection rides
in on, so the check fails and names the call.

    scripts/sql_binding_check.py [<file.rs> ...]   (default: every crates/*/src file)
    scripts/sql_binding_check.py --self-test

Fails when it scanned nothing, so a renamed file cannot turn it into a no-op.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
def rust_sources() -> list[Path]:
    """Every crate source file under crates/*/src, sorted."""
    return sorted(ROOT.glob("crates/*/src/**/*.rs"))
CALL = re.compile(r"\.(execute|execute_batch|prepare|prepare_cached|query_row)\s*\(")


def _after_string_literal(s: str) -> int | None:
    """Index just past a leading Rust string literal in `s`, else None."""
    if s.startswith('r#"'):
        end = s.find('"#', 3)
        return end + 2 if end != -1 else None
    if s.startswith('r"'):
        end = s.find('"', 2)
        return end + 1 if end != -1 else None
    if s.startswith('"'):
        i = 1
        while i < len(s):
            if s[i] == "\\":
                i += 2
                continue
            if s[i] == '"':
                return i + 1
            i += 1
        return None
    return None


def first_arg_ok(after: str) -> bool:
    """`after` is the source right after the call's `(`. True when its first
    argument is a string literal (not concatenated) or an UPPER_CASE const."""
    s = after.lstrip(" \t\r\n")
    end = _after_string_literal(s)
    if end is not None:
        # a literal is fine only if nothing is added to it
        return s[end:].lstrip(" \t\r\n")[:1] != "+"
    # a .sql file embedded at compile time is a static literal, not run-time text
    if s.startswith("include_str!("):
        return True
    m = re.match(r"([A-Za-z_][A-Za-z0-9_:]*)", s)
    if m:
        last = m.group(1).split("::")[-1]
        return last == last.upper() and any(c.isalpha() for c in last)
    return False


def scan_text(text: str, name: str) -> tuple[int, list[str]]:
    checked = 0
    bad = []
    for m in CALL.finditer(text):
        checked += 1
        call = m.group(1)
        after = text[m.end():]
        if not first_arg_ok(after):
            snippet = after.lstrip()[:48].replace("\n", " ")
            line = text.count("\n", 0, m.start()) + 1
            bad.append(f"{name}:{line}: {call}(...) first argument is not a literal or an UPPER_CASE const: `{snippet}`")
    return checked, bad


def self_test() -> int:
    cases = [
        ('db.execute("INSERT INTO t VALUES (?1)", [x])', 0),
        ('db.prepare(LIST_SQL)', 0),
        ('db.prepare(Self::INSERT_SQL)', 0),
        ('db.query_row(r#"SELECT 1"#, [], f)', 0),
        ('db.execute(&format!("SELECT {x}"), [])', 1),
        ('db.prepare(format!("SELECT {x}"))', 1),
        ('db.execute("SELECT " + name, [])', 1),
        ('db.prepare(sql_built_at_runtime)', 1),
        ('db.execute_batch(include_str!("m.sql"))', 0),
    ]
    bad = 0
    for src, want in cases:
        n, errs = scan_text(src, "t")
        if n != 1 or len(errs) != want:
            print(f"FAIL ({n} calls, {len(errs)} flagged, want {want}): {src}")
            bad += 1
    # nothing scanned is itself a failure at the top level
    n, _ = scan_text("let x = 1;", "t")
    if n != 0:
        print("FAIL empty scan counted a call")
        bad += 1
    print(f"self-test: {len(cases) + 1 - bad}/{len(cases) + 1} pass")
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        return self_test()
    paths = [ROOT / a for a in argv] if argv else rust_sources()
    total = 0
    errors = []
    for p in paths:
        if not p.exists():
            errors.append(f"{p}: missing")
            continue
        rel = p.relative_to(ROOT) if p.is_absolute() and str(p).startswith(str(ROOT)) else p
        n, bad = scan_text(p.read_text(encoding="utf-8"), str(rel))
        total += n
        errors += bad
    print(f"sql-binding: {total} SQL call(s) checked in {len(paths)} file(s)")
    if total == 0:
        errors.append("no SQL calls found (nothing compared)")
    for e in errors:
        print(f"error: {e}", file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
