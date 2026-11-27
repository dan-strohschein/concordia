"""Tests of tools/disk/sweep.py (`python3 -I tools/disk/sweep_test.py`)."""

import io
import os
import sys
import tempfile
import time
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import sweep  # noqa: E402

NOW = 1_800_000_000.0


def unit(target, crate, name, age_days, profile="debug"):
    """A unit folder with a fingerprint and an output file, everything `age_days` old."""
    path = os.path.join(target, profile, "build", crate, name)
    for sub in ["fingerprint", "out"]:
        os.makedirs(os.path.join(path, sub))
        with open(os.path.join(path, sub, "f"), "wb") as f:
            f.write(b"x" * 8192)
    stamp = NOW - age_days * sweep.DAY
    for root, dirs, files in os.walk(path, topdown=False):
        for name in files + dirs:
            os.utime(os.path.join(root, name), (stamp, stamp))
    os.utime(path, (stamp, stamp))
    return path


class Sweep(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.target = self.dir.name

    def tearDown(self):
        self.dir.cleanup()

    def test_only_units_older_than_the_cutoff_go(self):
        old = unit(self.target, "concordia-tests", "aaaa", 5)
        fresh = unit(self.target, "concordia-tests", "bbbb", 1)
        out = io.StringIO()
        freed = sweep.sweep(self.target, 3, dry_run=False, now=NOW, out=out)
        self.assertFalse(os.path.exists(old))
        self.assertTrue(os.path.exists(fresh))
        self.assertGreater(freed, 0)
        self.assertIn(f"removed {old}", out.getvalue())
        self.assertIn("freed", out.getvalue())

    def test_a_recently_rewritten_fingerprint_keeps_an_old_unit(self):
        path = unit(self.target, "serde", "cccc", 10)
        os.utime(os.path.join(path, "fingerprint", "f"), (NOW, NOW))
        self.assertEqual(sweep.stale_units(self.target, 3, NOW), [])

    def test_dry_run_removes_nothing(self):
        old = unit(self.target, "concordia-app", "dddd", 9, profile="release")
        out = io.StringIO()
        sweep.sweep(self.target, 3, dry_run=True, now=NOW, out=out)
        self.assertTrue(os.path.exists(old))
        self.assertIn(f"would remove {old}", out.getvalue())

    def test_folders_outside_the_build_dirs_are_never_touched(self):
        for kept in ["toolchain/slang/x", "wasm-fixtures/release/build/a/b", "debug/deps/x"]:
            path = os.path.join(self.target, kept)
            os.makedirs(path)
            os.utime(path, (NOW - 30 * sweep.DAY,) * 2)
        sweep.sweep(self.target, 3, dry_run=False, now=NOW, out=io.StringIO())
        for kept in ["toolchain/slang/x", "wasm-fixtures/release/build/a/b", "debug/deps/x"]:
            self.assertTrue(os.path.isdir(os.path.join(self.target, kept)), kept)

    def test_negative_days_are_refused(self):
        with self.assertRaises(ValueError):
            sweep.stale_units(self.target, -1, NOW)


if __name__ == "__main__":
    unittest.main()
