#!/usr/bin/env python3
"""追跡 file の本文と file 名に、特定の語 (製品の外部連携の名前) が許可リスト外で現れたら fail する

語そのものは平文で持たず SHA-256 で持つ (BANNED_WORD_HASHES)  本文を token
(`[A-Za-z0-9._/-]+` の連なり) に分け、token を英字の語に分解 (camelCase も分割、
小文字化) し、語の hash が集合にあれば、その token を「該当 token」とする

許可は file 単位: SHA-256(`<path>\\0<token 小文字>`) → 出現回数 (ALLOWED)
  - 許可リストに無い (path, token) が 1 つでもあれば fail
  - 許可した (path, token) の出現回数が一致しなければ fail (増えた / 消えた の両方、
    消えたら許可リストから外す)
  - 走査した file が 0 件なら fail
製品が実際に必要とする箇所 (外部 LLM の provider 名・model id、外部 agent 連携の
plugin と skill、その test) だけを許可する それ以外の箇所には書かない

走査対象: git が追跡している file と、追跡していないが ignore もされていない file
(`git ls-files --cached --others --exclude-standard`) ⇒ commit 前の file も local / preflight で捕まえる

想定する脅威: 人や agent が **うっかり** 名前を書くこと (大文字小文字・下線・camelCase・
他の語への埋め込みを含む) 文字列の連結・unicode escape・base64 等で **意図して** 隠す
書き方は対象外 (検出の規則を増やし続けても閉じないため、review で扱う)

usage: vocab_lint.py [--root <repo>] [--print-allowlist] [--self-test]
  --print-allowlist  現在の該当箇所から ALLOWED の行を出力する (追加は review の上で)
"""

from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

# 語の SHA-256 (小文字)
BANNED_WORD_HASHES = frozenset(
    {
        "c857d09db23e6822e3600bc06ad8d58f92ed62bc8efd81c753f77048662cb97d",
    }
)

# SHA-256(path + "\0" + token) → 出現回数
ALLOWED: dict[str, int] = {
    "3ffaa2d1b762200b6ed685b85f46d85f94aba454eeb132e0eedef61442ff682a": 1,
    "e842673242c68b054a45a9474b1bfb7d034f400808d91328cc17e7eb00fa02e9": 1,
    "938fe3b8cdc685f71f04da5f9a7e225edfddd6e89137a73eee378842cfaf329b": 2,
    "af71d5f0c663bec6ff86d3d6b437990f9926c10a54e64dbd01e8a812a29bd1ed": 1,
    "829919d90715c32ccc7231641b31cebb9f47b3ff50774434d078b7d737652680": 1,
    "a5048c9fd6603806b175feb184239bfada1ace37eebf811c75fc400b14b58a05": 2,
    "2fce1206037fe05fb65ee69462a31f0021e4d0f4cfe89f2325b6c00a72beed1f": 2,
    "65158952f001904697d616e98ecf6d7e3b8750a4f83cd44f36546c3bb2a37f9b": 1,
    "07d5ed334b0191c36be15d74a03d11bda798f8be8f71d6d315b36a4b3d4ba545": 2,
    "5e9759a32eeb52bfadaed64f1f2475cdef4c47bffa624a5e694593308c66d062": 4,
    "f98e891ee2270da9da008e312834e77bbac59b2e5daaa731c861de6ded7bada8": 2,
    "4db83ebb7f5ac214b07a4e66fc5f9625cecda85437c5a08204f059683e0a52de": 1,
    "9dad9563c9c9eb7bfd5a07d7eb1a4e6202dbcc5d4789f1250276892842edda02": 1,
    "b4d20128cfa175df8da52b570038fd8b905bd284db01ba797979abdee4549653": 1,
    "40f8027e044e8aa3e2783d55bd9628c9b073f4e82458878f6bbdc8fcb3f791ed": 1,
    "55f211338563b6fb7332ed8244c5a6173ccf26ab3c9276f58854d9424354d22b": 1,
    "5a47ba87b0813f42f563ae9ebeed0719bbf8d25346f07a087f9e54a0633261a0": 2,
    "22ea6988e9dfdfa84af663ad00af9c37e9318322101db990cb5b29e5d7fd1203": 1,
    "971a9ffc87554fbe3293e6ca79ca2ec3be9f6e9fecc4138fece5c35a75900ef7": 2,
}

TOKEN_RE = re.compile(r"[A-Za-z0-9._/-]+")
WORD_RE = re.compile(r"[A-Z]?[a-z]+|[A-Z]+(?![a-z])")


def sha(s: str) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def words(token: str) -> list[str]:
    return [w.lower() for w in WORD_RE.findall(token)]


def hits_in(text: str, banned: frozenset[str]) -> list[tuple[int, str]]:
    """(行番号, token 小文字) の一覧 行番号 0 は file 名"""
    out: list[tuple[int, str]] = []
    for i, line in enumerate(text.splitlines(), 1):
        for m in TOKEN_RE.finditer(line):
            tok = m.group(0)
            ws = words(tok)
            joined = "".join(ws)
            if any(sha(w) in banned for w in ws) or any(
                sha(joined[a:b]) in banned for a in range(len(joined)) for b in range(a + 1, len(joined) + 1)
                if b - a <= 16
            ):
                out.append((i, tok.lower()))
    return out


def scan(root: Path, banned: frozenset[str]) -> tuple[Counter, list[tuple[str, int, str]], int]:
    files = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z", "--cached", "--others", "--exclude-standard"], capture_output=True, text=True, check=True
    ).stdout.split("\0")
    counts: Counter = Counter()
    where: list[tuple[str, int, str]] = []
    scanned = 0
    for rel in filter(None, files):
        p = root / rel
        if not p.is_file():
            continue
        scanned += 1
        found = [(0, t) for _, t in hits_in(rel, banned)]
        data = p.read_bytes()
        if b"\0" not in data[:8192]:
            found += hits_in(data.decode("utf-8", errors="replace"), banned)
        for line, tok in found:
            counts[sha(f"{rel}\0{tok}")] += 1
            where.append((rel, line, tok))
    return counts, where, scanned


def check(root: Path, banned: frozenset[str], allowed: dict[str, int]) -> int:
    counts, where, scanned = scan(root, banned)
    errors: list[str] = []
    if scanned == 0:
        errors.append("走査した file が 0 件")
    reported: set[str] = set()
    for rel, line, tok in where:
        h = sha(f"{rel}\0{tok}")
        if h not in allowed and h not in reported:
            reported.add(h)
            loc = "file 名" if line == 0 else f"{line} 行"
            errors.append(f"{rel} ({loc}): 許可リストに無い token (pair {h[:12]})")
    for h, n in allowed.items():
        if counts.get(h, 0) != n:
            errors.append(f"許可した pair {h[:12]} の出現回数が {counts.get(h, 0)} (許可リストは {n})")
    print(f"vocab-lint: scanned {scanned} file(s), {sum(counts.values())} occurrence(s) in {len(counts)} pair(s)")
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    return 1 if errors else 0


def print_allowlist(root: Path, banned: frozenset[str]) -> int:
    counts, where, _ = scan(root, banned)
    path_of = {sha(f"{rel}\0{tok}"): rel for rel, _, tok in where}
    for h, n in sorted(counts.items(), key=lambda kv: (path_of[kv[0]], kv[0])):
        print(f'    "{h}": {n},')
        print(f"{path_of[h]}: {n}", file=sys.stderr)
    return 0


def self_test() -> int:
    """架空の語で歯を確かめる (実際の語は使わない)"""
    import tempfile

    fake = frozenset({sha("zorblax")})
    with tempfile.TemporaryDirectory() as tmp:
        t = Path(tmp)

        def w(rel: str, body: str) -> None:
            p = t / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(body, encoding="utf-8")

        def git_add() -> None:
            subprocess.run(["git", "-C", str(t), "add", "-A"], check=True, capture_output=True)

        subprocess.run(["git", "-C", str(t), "init", "-q"], check=True)
        w("src/provider.rs", 'const M: &str = "zorblax-large-2";\n// uses ZorblaxCode\n')
        w(".zorblax-plugin/plugin.json", "{}\n")
        w("README.md", "nothing here\n")
        git_add()
        counts, _, scanned = scan(t, fake)
        assert scanned == 3, scanned
        allowed = dict(counts)
        assert len(allowed) == 3, allowed  # 2 token in provider.rs + file 名
        assert check(t, fake, allowed) == 0, "許可した pair を red にした"

        w("docs/new.md", "see Zorblax for details\n")
        git_add()
        assert check(t, fake, allowed) == 1, "新しい file の新しい token を検出しない"
        (t / "docs/new.md").unlink()
        git_add()

        w("README.md", "now mentions zorblax\n")
        assert check(t, fake, allowed) == 1, "許可していない file への追加を検出しない"
        w("README.md", "nothing here\n")

        w("src/provider.rs", 'const M: &str = "zorblax-large-2";\n// uses ZorblaxCode\nconst N: &str = "zorblax-large-2";\n')
        assert check(t, fake, allowed) == 1, "許可した pair の出現回数の増加を検出しない"

        w("src/provider.rs", 'const M: &str = "zorblax-large-2";\n')
        assert check(t, fake, allowed) == 1, "許可した pair の消失 (許可リストの残骸) を検出しない"
        w("src/provider.rs", 'const M: &str = "zorblax-large-2";\n// uses ZorblaxCode\n')

        for label, body in {
            "大文字": "// ZORBLAX\n",
            "下線つき": "let zorblax_thing = 1;\n",
            "他の語に埋め込み": "// xzorblaxy\n",
            "camelCase": "struct MyZorblaxClient;\n",
        }.items():
            w("src/other.rs", body)
            git_add()
            assert check(t, fake, allowed) == 1, f"{label} の綴りを検出しない"
            (t / "src/other.rs").unlink()
            git_add()
        assert check(t, fake, allowed) == 0

        # 追跡していない (commit 前の) file も走査する ignore された file は対象外
        w("notes/draft.md", "zorblax draft\n")
        assert check(t, fake, allowed) == 1, "追跡していない file の token を検出しない"
        w(".gitignore", "notes/\n")
        git_add()
        _, _, n_ignored = scan(t, fake)
        allowed_with_ignore = dict(allowed)
        assert check(t, fake, allowed_with_ignore) == 0, "ignore された file を走査した"
        assert n_ignored == 4, n_ignored  # 3 file + .gitignore
        (t / ".gitignore").unlink()
        (t / "notes/draft.md").unlink()
        git_add()

        empty = Path(tmp) / "empty"
        empty.mkdir()
        subprocess.run(["git", "-C", str(empty), "init", "-q"], check=True)
        assert check(empty, fake, {}) == 1, "走査 0 件で fail しない"
    print("self-test ok")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    ap.add_argument("--print-allowlist", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if a.print_allowlist:
        return print_allowlist(a.root, BANNED_WORD_HASHES)
    return check(a.root, BANNED_WORD_HASHES, ALLOWED)


if __name__ == "__main__":
    sys.exit(main())
