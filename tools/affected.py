"""What a phase's checks run (`just check`; 2026-11-25): the tests of the crates this branch
changes and of every crate depending on them, never the milestone gates (they run with
`just check full` at a milestone's end) unless a gate file changed. Prints a JSON selection:
mode (`affected`, or `full` when a root manifest or lock changed), the changed packages, the
nextest filter, and whether the editor (its end-to-end tests only for more than regenerated
types), Tauri and supply-chain steps apply."""

import json
import os
import subprocess

# concordia-tests' one test binary: its tests are named after their file (`m1_physics_golden::`).
TESTS = "binary_id(concordia-tests::integration)"
# Milestone gates (tests/gates/m<n>[s]_*.rs).
GATES = f"({TESTS} & test(/^m[0-9]+s?_/))"
# Always run: it checks plans, workflows and crate docs, and takes under a second.
HYGIENE = f"({TESTS} & test(/^repo_hygiene::/))"
FULL_ON = {"Cargo.toml", "Cargo.lock"}
DENY_ON = {"deny.toml", "editor/src-tauri/Cargo.lock", "editor/src-tauri/deny.toml"}
# Types regenerated from the engine's schemas: the editor's type check and unit tests cover them.
GENERATED = "editor/ui/src/api/generated/"


def owner(path, packages):
    """The package whose folder most deeply contains `path`, if any."""
    best = None
    for name, folder in packages.items():
        if path.startswith(folder.rstrip("/") + "/") and (
            best is None or len(folder) > len(packages[best])
        ):
            best = name
    return best


def select(files, packages):
    """The selection for changed `files` (repo-relative); `packages` maps names to folders."""
    changed, gates = set(), set()
    for path in files:
        if path.startswith("tests/gates/") and path.endswith(".rs"):
            gates.add(os.path.basename(path)[: -len(".rs")])
            continue
        if path.startswith("tests/golden/"):
            # Goldens are read by the renderer's tests and the gates' non-gate siblings.
            changed.update({"concordia-render", "concordia-tests"} & packages.keys())
            continue
        name = owner(path, packages)
        if name is not None:
            changed.add(name)
    terms = [f"rdeps(={name})" for name in sorted(changed)]
    filter_ = f"(({' | '.join(terms)}) - {GATES})" if terms else ""
    extra = [HYGIENE] + [f"({TESTS} & test(/^{g}::/))" for g in sorted(gates)]
    return {
        "mode": "full" if FULL_ON & set(files) else "affected",
        "packages": sorted(changed),
        "filter": " | ".join(([filter_] if filter_ else []) + extra),
        "editor": any(p.startswith("editor/") for p in files),
        "editor_e2e": any(p.startswith("editor/") and not p.startswith(GENERATED) for p in files),
        "tauri": any(p.startswith("editor/src-tauri/") for p in files),
        "deny": bool(DENY_ON & set(files)),
    }


def changed_files():
    """Files this branch changes against origin/main, committed or not."""
    git = lambda *a: subprocess.run(["git", *a], capture_output=True, text=True,
                                    check=True).stdout
    base = git("merge-base", "HEAD", "origin/main").strip()
    names = git("diff", "--name-only", base).split() + git("ls-files", "--others",
                                                          "--exclude-standard").split()
    return sorted(set(names))


def workspace_packages():
    """Every workspace package's name and folder, repo-relative."""
    meta = json.loads(subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        capture_output=True, text=True, check=True).stdout)
    root = meta["workspace_root"].rstrip("/") + "/"
    return {p["name"]: os.path.dirname(p["manifest_path"])[len(root):] for p in meta["packages"]}


if __name__ == "__main__":
    print(json.dumps(select(changed_files(), workspace_packages())))
