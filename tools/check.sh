#!/usr/bin/env bash
# The checks, per phase or per milestone (author decision 2026-11-25: a phase waits at most about
# 7 minutes; the whole suite and the vulkan workflow run at a milestone's end).
#
#   just check        a phase: tools/affected.py picks the tests of the crates this branch changes
#                     and of their dependents, minus the milestone gates (unless a gate changed);
#                     clippy on the changed crates (all of `just lint` if a WASM guest changed);
#                     the editor, Tauri and deny steps only when their files changed. A changed
#                     root Cargo.toml or Cargo.lock falls back to the full checks.
#   just check full   a milestone: everything. Builds are memory-bound (an 18 GB Mac; one build at
#                     CARGO_BUILD_JOBS=6 stays under the 10 GB cap per command), so two builds
#                     never overlap: the fixtures and the workspace's test build come first; the
#                     side lane (lint on target/lint, the Tauri clippy, deny on every lock) runs
#                     with half the jobs while the workspace tests (the gates' results are
#                     summarised from this run) and doc tests execute; the editor's tests last.
#
# Logs and exit statuses go to target/check/; exits non-zero if any step failed. CHECK_EDITOR=0
# skips the editor's tests in either mode.
set -u
out=target/check
rm -rf "$out" && mkdir -p "$out"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}"
export CONCORDIA_REQUIRE_GPU="${CONCORDIA_REQUIRE_GPU:-1}"

# Runs "$@" as step $1, logging to target/check/<step>.log and recording its exit status.
step() {
    local name=$1
    shift
    local start=$SECONDS
    "$@" >"$out/$name.log" 2>&1
    local status=$?
    echo "$status $((SECONDS - start))" >"$out/$name.status"
    return "$status"
}

# Beside the tests, with half the build jobs: its builds and the running tests share the memory.
side() {
    local jobs=$(((CARGO_BUILD_JOBS + 1) / 2))
    step lint env CARGO_TARGET_DIR=target/lint CARGO_BUILD_JOBS="$jobs" just lint
    step tauri env CARGO_BUILD_JOBS="$jobs" cargo clippy --manifest-path editor/src-tauri/Cargo.toml --all-targets -- -D warnings
    step deny sh -c 'just deny && cargo deny --manifest-path editor/src-tauri/Cargo.toml --config editor/src-tauri/deny.toml check'
}

# WASM guest packages (their clippy needs the wasm target: `just lint` covers them).
GUESTS=" hello-guest concordia-script-guest conformance-guest nav-guest audio-guest input-guest wanderers lamplight-rules "

affected() {
    local selection mode filter packages
    step selector python3 -I tools/affected_test.py || return 1
    selection=$(python3 -I tools/affected.py) || return 1
    echo "$selection" >"$out/selection.json"
    field() { python3 -I -c "import json,sys; v=json.load(sys.stdin)[sys.argv[1]]; print(' '.join(v) if isinstance(v, list) else str(v).lower())" "$1" <<<"$selection"; }
    mode=$(field mode)
    if [ "$mode" = full ]; then
        echo "a root manifest or lock changed: running the full checks"
        return 2
    fi
    filter=$(field filter)
    packages=$(field packages)
    echo "changed packages: ${packages:-none}"
    echo "tests: $filter"
    step fixtures sh -c 'just wasm && just native-fixtures' || return 0
    local clippy=() guest=0 p
    for p in $packages; do
        case "$GUESTS" in *" $p "*) guest=1 ;; *) clippy+=(-p "$p") ;; esac
    done
    if [ "$guest" = 1 ]; then
        step lint just lint
    elif [ ${#clippy[@]} -gt 0 ]; then
        step lint sh -c "cargo fmt --all --check && cargo clippy ${clippy[*]} --all-targets --all-features -- -D warnings"
    else
        step lint cargo fmt --all --check
    fi
    step tests cargo nextest run --workspace --no-fail-fast --no-tests=pass -E "$filter"
    if [ ${#clippy[@]} -gt 0 ]; then
        step doc cargo test --doc "${clippy[@]}"
    fi
    if [ "$(field tauri)" = true ]; then
        step tauri cargo clippy --manifest-path editor/src-tauri/Cargo.toml --all-targets -- -D warnings
    fi
    if [ "$(field editor_e2e)" = true ] && [ "${CHECK_EDITOR:-1}" = 1 ]; then
        step editor just editor-test
    elif [ "$(field editor)" = true ] && [ "${CHECK_EDITOR:-1}" = 1 ]; then
        step editor sh -c 'cd editor/ui && pnpm install --frozen-lockfile --ignore-scripts && pnpm lint && pnpm test'
    fi
    if [ "$(field deny)" = true ]; then
        step deny sh -c 'just deny && cargo deny --manifest-path editor/src-tauri/Cargo.toml --config editor/src-tauri/deny.toml check'
    fi
    return 0
}

full() {
if step fixtures sh -c 'just wasm && just native-fixtures' &&
    step build cargo nextest run --workspace --no-run; then
    side &
    side_pid=$!
    step workspace cargo nextest run --workspace --no-fail-fast
    step doc cargo test --workspace --doc
    wait "$side_pid"
    if [ "${CHECK_EDITOR:-1}" = 1 ]; then
        step editor just editor-test
    fi
else
    side
fi
}

mode="${1:-affected}"
case "$mode" in
    affected)
        affected
        [ $? = 2 ] && full
        ;;
    full) full ;;
    *)
        echo "usage: tools/check.sh [affected|full]" >&2
        exit 2
        ;;
esac

failed=0
for status in "$out"/*.status; do
    [ -e "$status" ] || continue
    name=$(basename "$status" .status)
    read -r code seconds <"$status"
    printf '%-10s %s  (%ss)\n' "$name" "$([ "$code" = 0 ] && echo ok || echo "FAILED, see $out/$name.log")" "$seconds"
    [ "$code" = 0 ] || failed=1
done
if [ -f "$out/tests.log" ]; then
    grep -E '^ +Summary' "$out/tests.log" | tail -1
fi
if [ -f "$out/workspace.log" ]; then
    grep -E '^ +Summary' "$out/workspace.log" | tail -1
    # The gate binaries' results from the same run (tests/gates/m<n>_*.rs).
    for m in 0 1 2 3 4 5; do
        passed=$(grep -cE "^ +PASS .*concordia-tests::m${m}_" "$out/workspace.log")
        failing=$(grep -cE "^ +FAIL .*concordia-tests::m${m}_" "$out/workspace.log")
        [ $((passed + failing)) -gt 0 ] && echo "gate M$m: $passed passed, $failing failed"
    done
fi
exit "$failed"
