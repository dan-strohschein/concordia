"""Removes stale build units (`just prune-builds`; 2026-11-27).

A unit is a folder `target/<profile>/build/<crate>/<hash>/` that Cargo has not written to in the
last `--days` days (default 3); each source change leaves its predecessor behind. A unit removed
by mistake only costs a recompile. Prints each removal and the space freed; `--dry-run` only
lists. Usage: `python3 -I tools/disk/sweep.py [--dry-run] [--days N] [target]`.
"""

import argparse
import os
import shutil
import sys
import time

PROFILES = ("debug", "release")
DAY = 86400


def newest_mtime(unit):
    """The latest mtime of `unit`, its entries and theirs (Cargo rewrites fingerprints there)."""
    newest = os.lstat(unit).st_mtime
    for entry in os.scandir(unit):
        newest = max(newest, entry.stat(follow_symlinks=False).st_mtime)
        if entry.is_dir(follow_symlinks=False):
            for inner in os.scandir(entry.path):
                newest = max(newest, inner.stat(follow_symlinks=False).st_mtime)
    return newest


def size(path):
    """Bytes on disk under `path`, not following links."""
    total = 0
    for root, dirs, files in os.walk(path):
        for name in files:
            try:
                total += os.lstat(os.path.join(root, name)).st_blocks * 512
            except OSError:
                pass
    return total


def stale_units(target, days, now):
    """Unit folders under `target`'s build dirs older than `days` days, oldest first."""
    if days < 0:
        raise ValueError(f"days must be at least 0, got {days}")
    cutoff = now - days * DAY
    found = []
    for profile in PROFILES:
        build = os.path.join(target, profile, "build")
        if not os.path.isdir(build):
            continue
        for crate in sorted(os.scandir(build), key=lambda e: e.name):
            if not crate.is_dir(follow_symlinks=False):
                continue
            for unit in os.scandir(crate.path):
                if unit.is_dir(follow_symlinks=False):
                    mtime = newest_mtime(unit.path)
                    if mtime < cutoff:
                        found.append((mtime, unit.path))
    return [path for _, path in sorted(found)]


def sweep(target, days, dry_run, now=None, out=sys.stdout):
    """Removes (or lists) the stale units; returns the bytes freed (or that would be)."""
    freed = 0
    for unit in stale_units(target, days, time.time() if now is None else now):
        bytes_ = size(unit)
        if not dry_run:
            shutil.rmtree(unit)
        freed += bytes_
        print(f"{'would remove' if dry_run else 'removed'} {unit} ({bytes_ / 1e6:.0f} MB)", file=out)
    print(f"{'would free' if dry_run else 'freed'} {freed / 1e9:.1f} GB", file=out)
    return freed


def main(argv):
    parser = argparse.ArgumentParser(description="Remove stale Cargo build units.")
    parser.add_argument("target", nargs="?", default="target", help="the target folder")
    parser.add_argument("--days", type=float, default=3, help="keep units written this recently")
    parser.add_argument("--dry-run", action="store_true", help="list, remove nothing")
    args = parser.parse_args(argv)
    if args.days < 0:
        parser.error("--days must be at least 0")
    if not os.path.isdir(args.target):
        parser.error(f"no target folder at {args.target}")
    sweep(args.target, args.days, args.dry_run)


if __name__ == "__main__":
    main(sys.argv[1:])
