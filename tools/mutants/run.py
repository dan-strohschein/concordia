"""Mutation checks in parallel (`just mutants <file.json> [jobs]`; 2026-11-25).

Each mutant is `{"name", "file", "old", "new", "args"}`: `old` must occur exactly once in `file`
(repo-relative), and `args` are what follows `cargo nextest run` (a mutant is killed when those
tests fail). Workers run in git worktrees of the committed HEAD under target/mutants/ (so the tree
must be clean), each with its own target folder; the main target's toolchains and fixtures are
linked in. A mutated file is restored, and its mtime refreshed, before the next mutant, so cargo
never keeps a mutated build. Prints one line per mutant and writes target/mutants/results.json.
"""

import json
import os
import queue
import shutil
import subprocess
import sys
import threading
import time

ROOT = subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True,
                      check=True).stdout.strip()
BASE = os.path.join(ROOT, "target", "mutants")
# Build outputs stay per worker; everything else in target/ (toolchains, fixtures) is shared.
BUILD_DIRS = {"debug", "release", "lint", "mutants", "check", "tmp", "doc", "criterion"}
# A worker's debug build of the render or gate crates takes about this much disk.
GB_PER_WORKER = 20


def git(*args, cwd=ROOT):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, check=True).stdout


def check_inputs(mutants):
    """Every mutant names an existing file whose `old` text occurs exactly once."""
    problems = []
    for m in mutants:
        path = os.path.join(ROOT, m["file"])
        count = open(path).read().count(m["old"]) if os.path.isfile(path) else -1
        if count != 1:
            problems.append(f"{m['name']}: {m['file']} has {count} matches of its old text")
    return problems


def worktree(index):
    """A fresh worktree of HEAD for worker `index`, with the shared target entries linked in."""
    path = os.path.join(BASE, f"w{index}")
    if os.path.exists(path):
        git("worktree", "remove", "--force", path)
    git("worktree", "add", "--detach", path, "HEAD")
    os.makedirs(os.path.join(path, "target"), exist_ok=True)
    for entry in os.listdir(os.path.join(ROOT, "target")):
        if entry not in BUILD_DIRS:
            os.symlink(os.path.join(ROOT, "target", entry), os.path.join(path, "target", entry))
    return path


def nextest(tree, target, jobs, args):
    """Runs `cargo nextest run args` in `tree`: (exit code or None on timeout, output)."""
    env = dict(os.environ, CARGO_TARGET_DIR=target, CARGO_BUILD_JOBS=str(jobs),
               CONCORDIA_REQUIRE_GPU=os.environ.get("CONCORDIA_REQUIRE_GPU", "1"))
    try:
        r = subprocess.run(["cargo", "nextest", "run", *args, "--no-fail-fast"], cwd=tree,
                           env=env, capture_output=True, text=True, timeout=3600)
        return r.returncode, r.stdout + r.stderr
    except subprocess.TimeoutExpired:
        return None, ""


def run_mutant(tree, target, jobs, m):
    """Applies `m` in `tree`, runs its tests and restores the file; returns the verdict."""
    path = os.path.join(tree, m["file"])
    text = open(path).read()
    open(path, "w").write(text.replace(m["old"], m["new"]))
    try:
        code, out = nextest(tree, target, jobs, m["args"])
        if code is None:
            return "killed (timeout)"
        if code == 0:
            return "SURVIVED"
        if "error[E" in out or "could not compile" in out:
            return "COMPILE ERROR"
        return "killed"
    finally:
        open(path, "w").write(text)
        os.utime(path, None)
        time.sleep(1)
        os.utime(path, None)


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: run.py <mutants.json> [workers]")
    mutants = json.load(open(sys.argv[1]))
    workers = int(sys.argv[2]) if len(sys.argv) > 2 else 3
    if workers < 1:
        sys.exit("workers must be at least 1")
    if git("status", "--porcelain", "--untracked-files=no").strip():
        sys.exit("commit first: workers test the committed HEAD")
    problems = check_inputs(mutants)
    if problems:
        sys.exit("\n".join(problems))
    free_gb = shutil.disk_usage(ROOT).free / 1e9
    if free_gb < GB_PER_WORKER * workers + 25:
        sys.exit(f"{free_gb:.0f} GB free: {workers} workers need about "
                 f"{GB_PER_WORKER * workers + 25} GB (fewer workers, or `just clean-builds`)")
    jobs = max(1, (os.cpu_count() or 4) // workers)
    todo = queue.Queue()
    for m in mutants:
        todo.put(m)
    results, lock = [], threading.Lock()
    # Each distinct test selection runs once unmutated first: a failing baseline (a broken
    # setup, a missing fixture) would otherwise count every mutant as killed.
    baselines = {}

    def worker(index):
        tree = worktree(index)
        target = os.path.join(BASE, f"target-{index}")
        while True:
            try:
                m = todo.get_nowait()
            except queue.Empty:
                return
            key = tuple(m["args"])
            with lock:
                baseline = baselines.get(key)
            if baseline is None:
                baseline = nextest(tree, target, jobs, m["args"])[0] == 0
                with lock:
                    baselines[key] = baseline
            verdict = run_mutant(tree, target, jobs, m) if baseline else "NOT RUN: its tests fail unmutated"
            with lock:
                results.append({"name": m["name"], "verdict": verdict})
                print(f"{m['name']}: {verdict}", flush=True)

    threads = [threading.Thread(target=worker, args=(i,)) for i in range(workers)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    for i in range(workers):
        git("worktree", "remove", "--force", os.path.join(BASE, f"w{i}"))
    json.dump(results, open(os.path.join(BASE, "results.json"), "w"), indent=1)
    survived = [r["name"] for r in results if r["verdict"] != "killed"
                and not r["verdict"].startswith("killed")]
    print(f"{len(results) - len(survived)} of {len(results)} killed")
    for name in survived:
        print(f"  not killed: {name}")


if __name__ == "__main__":
    main()
