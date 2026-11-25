#!/usr/bin/env bash
# The end-of-phase checklist with independent steps at once (`just check`; 2026-11-25).
# Main lane, on target/debug: wasm and native fixtures, one workspace run (the gate binaries
# included: their results are summarised from it), doc tests, then the editor's tests (they reuse
# its CLI build). Side lane, beside it on its own target folders and without the GPU: lint
# (target/lint), the Tauri clippy (the editor's target) and deny on every lock. Logs and exit
# statuses go to target/check/. Exits non-zero if any step failed. CHECK_EDITOR=0 skips the
# editor's tests; CARGO_BUILD_JOBS defaults to 10.
set -u
out=target/check
rm -rf "$out" && mkdir -p "$out"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-10}"
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

side() {
    step lint env CARGO_TARGET_DIR=target/lint just lint
    step tauri cargo clippy --manifest-path editor/src-tauri/Cargo.toml --all-targets -- -D warnings
    step deny sh -c 'just deny && cargo deny --manifest-path editor/src-tauri/Cargo.toml --config editor/src-tauri/deny.toml check'
}

side &
side_pid=$!
if step fixtures sh -c 'just wasm && just native-fixtures'; then
    step workspace cargo nextest run --workspace --no-fail-fast
    step doc cargo test --workspace --doc
    if [ "${CHECK_EDITOR:-1}" = 1 ]; then
        step editor just editor-test
    fi
fi
wait "$side_pid"

failed=0
for status in "$out"/*.status; do
    name=$(basename "$status" .status)
    read -r code seconds <"$status"
    printf '%-10s %s  (%ss)\n' "$name" "$([ "$code" = 0 ] && echo ok || echo "FAILED, see $out/$name.log")" "$seconds"
    [ "$code" = 0 ] || failed=1
done
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
