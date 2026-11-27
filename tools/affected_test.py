"""Tests of tools/affected.py's selection (`python3 -I tools/affected_test.py`)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import affected  # noqa: E402

PACKAGES = {
    "concordia-render": "crates/concordia-render",
    "concordia-rhi": "crates/concordia-rhi",
    "concordia-tests": "tests",
    "hello-guest": "tests/fixtures/wasm/hello-guest",
}


def select(*files):
    return affected.select(list(files), PACKAGES)


class Selection(unittest.TestCase):
    def test_changed_crates_and_their_dependents_run_without_the_gates(self):
        s = select("crates/concordia-render/src/renderer.rs", "crates/concordia-rhi/src/caps.rs")
        self.assertEqual(s["mode"], "affected")
        self.assertEqual(s["packages"], ["concordia-render", "concordia-rhi"])
        self.assertIn("rdeps(=concordia-render)", s["filter"])
        self.assertIn("rdeps(=concordia-rhi)", s["filter"])
        self.assertIn("- (binary_id(concordia-tests::integration) & test(/^m[0-9]+s?_/))",
                      s["filter"])
        self.assertIn("(binary_id(concordia-tests::integration) & test(/^repo_hygiene::/))",
                      s["filter"])

    def test_the_deepest_package_folder_owns_a_file(self):
        s = select("tests/fixtures/wasm/hello-guest/src/lib.rs")
        self.assertEqual(s["packages"], ["hello-guest"])

    def test_a_changed_gate_runs_that_gate(self):
        s = select("tests/gates/m1_physics_golden.rs")
        self.assertIn(
            "(binary_id(concordia-tests::integration) & test(/^m1_physics_golden::/))", s["filter"])

    def test_goldens_rerun_the_renderer(self):
        s = select("tests/golden/m4_motion/view_motion.png")
        self.assertIn("concordia-render", s["packages"])

    def test_docs_and_tooling_run_only_the_hygiene_test(self):
        s = select("plans/M4-unreal-class-visuals.md", ".github/workflows/vulkan.yml", "justfile")
        self.assertEqual(s["packages"], [])
        self.assertEqual(s["filter"],
                         "(binary_id(concordia-tests::integration) & test(/^repo_hygiene::/))")

    def test_root_manifests_and_locks_mean_the_full_suite(self):
        for root in ["Cargo.toml", "Cargo.lock"]:
            self.assertEqual(select(root)["mode"], "full", root)

    def test_generated_types_need_the_editors_unit_tests_only(self):
        s = select("editor/ui/src/api/generated/MethodRenderStatsResult.ts")
        self.assertTrue(s["editor"])
        self.assertFalse(s["editor_e2e"])
        for real in ["editor/ui/src/viewport/tools.ts", "editor/ui/tests/e2e/view.spec.ts",
                     "editor/src-tauri/src/main.rs"]:
            self.assertTrue(select(real)["editor_e2e"], real)
        self.assertFalse(select("crates/concordia-rhi/src/caps.rs")["editor_e2e"])

    def test_editor_and_supply_chain_steps_follow_their_files(self):
        self.assertTrue(select("editor/ui/src/viewport/tools.ts")["editor"])
        self.assertFalse(select("crates/concordia-rhi/src/caps.rs")["editor"])
        self.assertTrue(select("editor/src-tauri/src/main.rs")["tauri"])
        self.assertTrue(select("editor/src-tauri/Cargo.lock")["deny"])
        self.assertTrue(select("deny.toml")["deny"])
        self.assertFalse(select("crates/concordia-rhi/src/caps.rs")["deny"])


if __name__ == "__main__":
    unittest.main()
