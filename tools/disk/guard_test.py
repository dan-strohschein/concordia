"""Tests of tools/disk/guard.sh (`python3 -I tools/disk/guard_test.py`): it measures the disk
that holds target/ (which may be a symlink to an external drive) and stops builds when that
drive is missing."""

import json
import os
import subprocess
import tempfile
import unittest

GUARD = os.path.join(os.path.dirname(os.path.abspath(__file__)), "guard.sh")
BUILD = json.dumps({"tool_input": {"command": "cargo build -p concordia-core"}})


def run(repo, command=BUILD, **env):
    full = dict(os.environ, CLAUDE_PROJECT_DIR=repo, DISK_STOP_GB="0", DISK_WARN_GB="0")
    full.update(env)
    return subprocess.run(["bash", GUARD], input=command, capture_output=True, text=True, env=full)


class Guard(unittest.TestCase):
    def test_a_build_with_a_local_target_passes(self):
        with tempfile.TemporaryDirectory() as repo:
            os.mkdir(os.path.join(repo, "target"))
            self.assertEqual(run(repo).returncode, 0)

    def test_a_target_on_a_missing_drive_stops_builds(self):
        with tempfile.TemporaryDirectory() as repo:
            os.symlink("/Volumes/no-such-drive/concordia-target", os.path.join(repo, "target"))
            result = run(repo)
            self.assertEqual(result.returncode, 2)
            self.assertIn("no-such-drive", result.stderr)

    def test_a_symlinked_target_is_measured_where_it_points(self):
        with tempfile.TemporaryDirectory() as repo, tempfile.TemporaryDirectory() as drive:
            os.symlink(drive, os.path.join(repo, "target"))
            # Stop below an impossible amount of free space: the message names the target's disk.
            result = run(repo, DISK_STOP_GB="999999999")
            self.assertEqual(result.returncode, 2)
            self.assertIn(os.path.realpath(drive), result.stderr)

    def test_commands_that_do_not_build_are_never_stopped(self):
        with tempfile.TemporaryDirectory() as repo:
            os.symlink("/Volumes/no-such-drive/t", os.path.join(repo, "target"))
            ls = json.dumps({"tool_input": {"command": "ls"}})
            self.assertEqual(run(repo, command=ls).returncode, 0)


if __name__ == "__main__":
    unittest.main()
