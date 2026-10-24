#!/usr/bin/env python3
"""Tests of scripts/deny_exception_guard.py (fixtures: a package list as `cargo metadata` gives it and a
deny.toml as a dict; the expected findings are written by hand from the rule, not by the guard)."""
from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import deny_exception_guard as g  # noqa: E402

ALLOW = ["MIT", "Apache-2.0", "BSD-3-Clause"]


def pkg(name, lic, version="1.0.0"):
    return {"name": name, "version": version, "license": lic}


def deny(*exceptions, allow=ALLOW):
    return {"licenses": {"allow": list(allow),
                         "exceptions": [{"name": n, "allow": list(a)} for n, a in exceptions]}}


AGPL_OR_COM = "AGPL-3.0-or-later OR LicenseRef-Commercial"
OK_PKGS = [pkg("alice-physics", AGPL_OR_COM), pkg("alice-print", "LicenseRef-Proprietary"),
           pkg("alice-zip", "Apache-2.0"), pkg("serde", "MIT OR Apache-2.0")]
OK_DENY = deny(("alice-physics", ["AGPL-3.0-or-later"]), ("alice-print", ["LicenseRef-Proprietary"]))


class Rule(unittest.TestCase):
    def test_exceptions_equal_to_the_declarations_pass(self):
        errs, n = g.check(OK_PKGS, OK_DENY, "alice-")
        self.assertEqual(errs, [])
        self.assertEqual(n, 2 + 3)  # 2 exceptions + 3 alice packages

    def test_an_id_the_crate_does_not_declare_is_looser(self):
        d = deny(("alice-physics", ["AGPL-3.0-or-later", "AGPL-3.0-only"]),
                 ("alice-print", ["LicenseRef-Proprietary"]))
        errs, _ = g.check(OK_PKGS, d, "alice-")
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("alice-physics 1.0.0: looser exception: allows ['AGPL-3.0-only']", errs[0])

    def test_an_exception_for_a_crate_that_is_not_a_dependency_is_stale(self):
        d = deny(*[(e["name"], e["allow"]) for e in OK_DENY["licenses"]["exceptions"]],
                 ("alice-gone", ["AGPL-3.0-only"]))
        errs, _ = g.check(OK_PKGS, d, "alice-")
        self.assertEqual(errs, ["alice-gone: stale exception (not a dependency)"])

    def test_an_exception_the_global_list_makes_unneeded_is_stale(self):
        # alice-zip declares Apache-2.0, which the global list accepts
        d = deny(*[(e["name"], e["allow"]) for e in OK_DENY["licenses"]["exceptions"]],
                 ("alice-zip", ["Apache-2.0"]))
        errs, _ = g.check(OK_PKGS, d, "alice-")
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("alice-zip 1.0.0: stale exception: the global allow list already accepts", errs[0])

    def test_a_prefixed_crate_without_the_exception_it_needs_is_missing(self):
        d = deny(("alice-physics", ["AGPL-3.0-or-later"]))
        errs, _ = g.check(OK_PKGS, d, "alice-")
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("alice-print 1.0.0: missing exception", errs[0])

    def test_a_relicensed_pin_names_the_exception_to_change(self):
        # the pin moves alice-physics to a commit that declares LicenseRef-Proprietary, deny.toml unchanged
        pkgs = [pkg("alice-physics", "LicenseRef-Proprietary")] + OK_PKGS[1:]
        errs, _ = g.check(pkgs, OK_DENY, "alice-")
        self.assertEqual(len(errs), 2, errs)
        self.assertTrue(any("alice-physics 1.0.0: looser exception: allows ['AGPL-3.0-or-later']" in e for e in errs))
        self.assertTrue(any("alice-physics 1.0.0: missing exception" in e for e in errs))
        # and the same commit's deny.toml change makes it pass
        d = deny(("alice-physics", ["LicenseRef-Proprietary"]), ("alice-print", ["LicenseRef-Proprietary"]))
        self.assertEqual(g.check(pkgs, d, "alice-")[0], [])

    def test_unprefixed_crates_are_not_required_to_have_exceptions(self):
        errs, _ = g.check(OK_PKGS + [pkg("ring", "LicenseRef-ring")], OK_DENY, "alice-")
        self.assertEqual(errs, [])

    def test_every_version_of_a_crate_is_checked(self):
        pkgs = OK_PKGS + [pkg("alice-physics", "AGPL-3.0-only OR LicenseRef-Commercial", "2.0.0")]
        errs, _ = g.check(pkgs, OK_DENY, "alice-")
        self.assertTrue(any("alice-physics 2.0.0: looser" in e for e in errs), errs)
        self.assertTrue(any("alice-physics 2.0.0: missing" in e for e in errs), errs)

    def test_a_crate_without_an_spdx_declaration_is_a_finding(self):
        errs, _ = g.check([pkg("alice-x", None)], deny(), "alice-")
        self.assertEqual(len(errs), 1, errs)
        self.assertIn("declares no SPDX licence", errs[0])

    def test_an_exception_without_a_name_or_ids_is_a_finding(self):
        d = {"licenses": {"allow": ALLOW, "exceptions": [{"name": "alice-physics", "allow": []}]}}
        errs, _ = g.check(OK_PKGS[:1], d, "alice-")
        self.assertTrue(any("empty allow list" in e for e in errs), errs)

    def test_the_crate_key_names_an_exception_too(self):
        d = {"licenses": {"allow": ALLOW, "exceptions": [{"crate": "alice-physics", "allow": ["AGPL-3.0-or-later"]}]}}
        self.assertEqual(g.check(OK_PKGS[:1], d, "alice-")[0], [])


class Expressions(unittest.TestCase):
    def test_deprecated_ids_read_as_their_current_form(self):
        # AGPL-3.0 (deprecated) is AGPL-3.0-only, on either side
        d = deny(("alice-a", ["AGPL-3.0"]))
        self.assertEqual(g.check([pkg("alice-a", "AGPL-3.0-only")], d, "alice-")[0], [])
        d = deny(("alice-a", ["AGPL-3.0-only"]))
        self.assertEqual(g.check([pkg("alice-a", "AGPL-3.0")], d, "alice-")[0], [])

    def test_or_and_with_and_parentheses(self):
        t = g.parse("(MIT OR Apache-2.0) AND Apache-2.0 WITH LLVM-exception")
        self.assertEqual(g.terms(t), {"MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception"})
        self.assertTrue(g.satisfied(t, {"MIT", "Apache-2.0 WITH LLVM-exception"}))
        self.assertFalse(g.satisfied(t, {"MIT", "Apache-2.0"}))

    def test_the_old_slash_form_is_an_or(self):
        self.assertTrue(g.satisfied(g.parse("MIT/Apache-2.0"), {"Apache-2.0"}))

    def test_a_malformed_expression_is_refused(self):
        for e in ("", "MIT OR", "(MIT", "MIT)", "AND MIT", "MIT WITH"):
            with self.subTest(e=e):
                with self.assertRaises(g.ExprError):
                    g.parse(e)


class Main(unittest.TestCase):
    def run_main(self, pkgs, d_text):
        with tempfile.TemporaryDirectory() as tmp:
            m = Path(tmp) / "m.json"
            m.write_text(json.dumps({"packages": pkgs}), encoding="utf-8")
            t = Path(tmp) / "deny.toml"
            t.write_text(d_text, encoding="utf-8")
            return g.main(["--metadata-json", str(m), "--deny", str(t)])

    TOML_OK = ('[licenses]\nallow = ["MIT", "Apache-2.0"]\n'
               '[[licenses.exceptions]]\nname = "alice-physics"\nallow = ["AGPL-3.0-or-later"]\n')

    def test_main_passes_and_fails(self):
        self.assertEqual(self.run_main(OK_PKGS[:1], self.TOML_OK), 0)
        self.assertEqual(self.run_main([pkg("alice-physics", "LicenseRef-Proprietary")], self.TOML_OK), 1)

    def test_nothing_compared_fails(self):
        self.assertEqual(self.run_main([pkg("serde", "MIT")], '[licenses]\nallow = ["MIT"]\n'), 1)

    def test_unreadable_input_is_an_error(self):
        self.assertEqual(self.run_main(OK_PKGS, "[licenses\n"), 2)


if __name__ == "__main__":
    unittest.main()
