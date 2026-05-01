#!/usr/bin/env bash
# Test the container logs feature end-to-end.
#
# Tests the ProcessBackend behaviour directly in the build VM using the
# pelagos CLI, which is exactly what ProcessBackend::stream_logs calls.
# Then runs cargo clippy in the VM to verify the Rust backend compiles
# clean on Linux.
#
# Prerequisites:
#   - Build VM running: pelagos --profile build ping
#   - pelagos binary built in the VM: /mnt/Projects/pelagos/target/debug/pelagos

set -euo pipefail

PELAGOS="pelagos --profile build"
VM_PELAGOS="/mnt/Projects/pelagos/target/debug/pelagos"
PASS=0
FAIL=0

green() { printf '\033[32m[PASS]\033[0m %s\n' "$*"; PASS=$((PASS+1)); }
red()   { printf '\033[31m[FAIL]\033[0m %s\n' "$*"; FAIL=$((FAIL+1)); }
info()  { printf '\033[34m[----]\033[0m %s\n' "$*"; }

vm() { $PELAGOS vm ssh -- "$@" 2>&1; }
vm_bg() { $PELAGOS vm ssh -- "$@" & }

cleanup() {
    info "Cleaning up test containers..."
    vm "$VM_PELAGOS rm --force logs-static  2>/dev/null || true"
    vm "$VM_PELAGOS rm --force logs-follow  2>/dev/null || true"
    vm "$VM_PELAGOS rm --force logs-empty   2>/dev/null || true"
}
trap cleanup EXIT

# ── 0. Preflight ─────────────────────────────────────────────────────────────

info "Checking VM is reachable..."
if ! $PELAGOS ping >/dev/null 2>&1; then
    red "Build VM not reachable — run: pelagos --profile build ping"
    exit 1
fi
green "VM reachable"

info "Checking pelagos binary..."
if ! vm "$VM_PELAGOS --version" >/dev/null; then
    red "pelagos binary not found at $VM_PELAGOS"
    exit 1
fi
green "pelagos binary present"

# ── 1. Static logs (no follow) ────────────────────────────────────────────────

info "Test 1: static logs from an exited container"
cleanup >/dev/null 2>&1 || true

# Run a container that prints 5 known lines then exits
vm "$VM_PELAGOS run --name logs-static --detach -- alpine sh -c \
    'for i in 1 2 3 4 5; do echo \"line-\$i\"; done'" >/dev/null

# Give it a moment to run and exit
sleep 2

LOG_OUT=$(vm "$VM_PELAGOS logs logs-static")
for i in 1 2 3 4 5; do
    if echo "$LOG_OUT" | grep -q "line-$i"; then
        green "  line-$i present in static logs"
    else
        red "  line-$i MISSING from static logs (got: $LOG_OUT)"
    fi
done

# ── 2. Follow mode — output arrives in real time ──────────────────────────────

info "Test 2: follow mode streams live output"

# Start a container that prints one line per second for 6 seconds
vm "$VM_PELAGOS run --name logs-follow --detach -- alpine sh -c \
    'for i in 1 2 3 4 5 6; do echo \"follow-line-\$i\"; sleep 1; done'" >/dev/null

sleep 1

# Follow for 3 seconds, then kill the follow process
FOLLOW_OUT=$(vm "timeout 3 $VM_PELAGOS logs --follow logs-follow || true")

FOLLOW_COUNT=$(echo "$FOLLOW_OUT" | grep -c "follow-line-" || true)
if [ "$FOLLOW_COUNT" -ge 2 ]; then
    green "Follow mode: received $FOLLOW_COUNT lines in ~3s (expected >=2)"
else
    red "Follow mode: only received $FOLLOW_COUNT lines in ~3s"
fi

# Verify follow terminates after the container exits
sleep 5  # container should have exited by now
FOLLOW_COMPLETE=$(vm "timeout 5 $VM_PELAGOS logs --follow logs-follow || true")
ALL_LINES=$(echo "$FOLLOW_COMPLETE" | grep -c "follow-line-" || true)
if [ "$ALL_LINES" -eq 6 ]; then
    green "Follow mode: all 6 lines present after container exit"
else
    red "Follow mode: expected 6 lines after exit, got $ALL_LINES"
fi

# ── 3. Empty / no-output container ───────────────────────────────────────────

info "Test 3: logs on container with no output"
vm "$VM_PELAGOS run --name logs-empty --detach -- alpine sh -c 'sleep 1'" >/dev/null
sleep 2
EMPTY_OUT=$(vm "$VM_PELAGOS logs logs-empty")
if [ -z "$EMPTY_OUT" ]; then
    green "Empty container: logs returns empty output (correct)"
else
    red "Empty container: expected empty output, got: $EMPTY_OUT"
fi

# ── 4. Clippy on Linux (ProcessBackend) ──────────────────────────────────────

info "Test 4: cargo clippy on Linux (validates ProcessBackend)"
if vm "cd /mnt/Projects/pelagos-ui/src-tauri && \
        cargo clippy -- -D warnings 2>&1 | grep -E '^error'" | grep -q error; then
    red "clippy: errors found"
else
    green "clippy: clean"
fi

# ── 5. cargo test (unit tests) ───────────────────────────────────────────────

info "Test 5: cargo test in VM"
TEST_OUT=$(vm "cd /mnt/Projects/pelagos-ui/src-tauri && cargo test 2>&1 | tail -5")
if echo "$TEST_OUT" | grep -qE "^test result: ok"; then
    green "cargo test: all tests pass"
else
    red "cargo test: unexpected output — $TEST_OUT"
fi

# ── Summary ──────────────────────────────────────────────────────────────────

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
