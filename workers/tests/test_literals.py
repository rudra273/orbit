import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from orbit_workers.literals import detect_literals  # noqa: E402


class DetectLiterals(unittest.TestCase):
    def test_every_literal_is_a_byte_copy_of_its_span(self):
        text = "Déjà vu: run `cargo test -p orbit-core -- --nocapture` on PostgreSQL 18.1, see https://example.com/docs."
        raw = text.encode("utf-8")

        literals = detect_literals(text)

        self.assertTrue(literals)
        for lit in literals:
            self.assertEqual(raw[lit.byte_start:lit.byte_end].decode("utf-8"), lit.exact)

    def test_finds_each_kind(self):
        text = "run `make db` with v0.8.1, see https://example.com/a."

        found = {(lit.kind, lit.exact) for lit in detect_literals(text)}

        self.assertEqual(found, {("command", "make db"), ("version", "v0.8.1"), ("url", "https://example.com/a")})

    def test_a_version_inside_a_command_is_not_reported_twice(self):
        literals = detect_literals("use `pip install foo==1.2.3` please")

        self.assertEqual([(lit.kind, lit.exact) for lit in literals], [("command", "pip install foo==1.2.3")])


if __name__ == "__main__":
    unittest.main()
