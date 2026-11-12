#!/usr/bin/env python3
"""Tests of scripts/alice_llm_dir.py on metadata documents of known shape."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import alice_llm_dir as m  # noqa: E402

SRC = "git+https://github.com/ext-sakamoro/ALICE-LLM?branch=main#" + "a" * 40


def meta(*pkgs):
    return {"packages": [{"name": n, "source": s, "manifest_path": str(p)} for n, s, p in pkgs]}


class Dir(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "alice-llm-x" / "abc1234"
        (self.root / "crates" / "core").mkdir(parents=True)
        for f in ("Cargo.toml", "Cargo.lock"):
            (self.root / f).write_text("")
        (self.root / "crates" / "core" / "Cargo.toml").write_text("")

    def tearDown(self):
        self.tmp.cleanup()

    def test_the_root_package_gives_its_directory(self):
        self.assertEqual(m.checkout_dir(meta(("alice-llm", SRC, self.root / "Cargo.toml"))), self.root)

    def test_a_member_walks_up_to_the_lock(self):
        p = self.root / "crates" / "core" / "Cargo.toml"
        self.assertEqual(m.checkout_dir(meta(("alice-llm", SRC, p))), self.root)

    def test_none_or_two_fail(self):
        with self.assertRaises(ValueError):
            m.checkout_dir(meta(("serde", None, self.root / "Cargo.toml")))
        with self.assertRaises(ValueError):
            m.checkout_dir(meta(("alice-llm", SRC, self.root / "Cargo.toml"), ("alice-llm", SRC, self.root / "Cargo.toml")))

    def test_a_path_registry_rev_or_lookalike_source_fails(self):
        for src in (None, "registry+https://github.com/rust-lang/crates.io-index",
                    "git+https://github.com/ext-sakamoro/ALICE-LLM?rev=" + "a" * 40 + "#" + "a" * 40,
                    "git+https://github.com/ext-sakamoro/ALICE-LLM-x?branch=main#" + "a" * 40):
            with self.assertRaises(ValueError, msg=src):
                m.checkout_dir(meta(("alice-llm", src, self.root / "Cargo.toml")))

    def test_main_exit_codes(self):
        d = Path(self.tmp.name)
        f = d / "m.json"
        f.write_text(json.dumps(meta(("alice-llm", SRC, self.root / "Cargo.toml"))))
        self.assertEqual(m.main(["--metadata", str(f)]), 0)
        f.write_text(json.dumps(meta()))
        self.assertEqual(m.main(["--metadata", str(f)]), 1)
        f.write_text("{")
        self.assertEqual(m.main(["--metadata", str(f)]), 2)


if __name__ == "__main__":
    unittest.main()
