#!/usr/bin/env bash
# 同梱の sidecar の寿命と待ち受けを実 binary で確かめる (ci.yml と preflight の共通)
#
#   1. Cargo.lock が固定する ALICE-LLM の commit (cargo が取った git checkout、
#      scripts/alice_llm_dir.py) から alice-llm-server を build
#   2. crates/llm/tests/sidecar_lifecycle.rs の ignore 付き試験を走らせる
#      (app 役の process を SIGTERM / SIGKILL で止め、sidecar の process と port が消えること)
#   3. 走った試験の数を確かめる (3 本、helper 1 + 試験 2) 0 本や feature 違いの空振りを失敗にする
set -euo pipefail

PY=$(command -v python3 || command -v python)
"$PY" scripts/test_alice_llm_dir.py
if [ -z "${ALICE_LLM_DIR:-}" ]; then
    ALICE_LLM_DIR="$("$PY" scripts/alice_llm_dir.py)"
fi
if [ ! -d "$ALICE_LLM_DIR" ]; then
    echo "ERROR: ALICE-LLM checkout not found at $ALICE_LLM_DIR" >&2
    exit 1
fi
# the checkout cargo made is shared: build into this workspace's target, not into the checkout
ALICE_LLM_TARGET="$(pwd)/target/alice-llm-server"
# ALICE-LLM's own lock names dependencies this workspace's fetch did not take: fetch them (online)
(cd "$ALICE_LLM_DIR" && CARGO_NET_OFFLINE=false cargo fetch --locked)
(cd "$ALICE_LLM_DIR" && CARGO_TARGET_DIR="$ALICE_LLM_TARGET" cargo build --locked --features server --bin alice-llm-server)
ALICE_LLM_SERVER_BIN="$ALICE_LLM_TARGET/debug/alice-llm-server"
export ALICE_LLM_SERVER_BIN
if [ ! -x "$ALICE_LLM_SERVER_BIN" ]; then
    echo "ERROR: built binary not found at $ALICE_LLM_SERVER_BIN" >&2
    exit 1
fi

log="$(mktemp)"
trap 'rm -f "$log"' EXIT
set +e
cargo test --locked -p text-to-print-llm --test sidecar_lifecycle -- --ignored --test-threads=1 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
set -e
if [ "$status" -ne 0 ]; then
    exit "$status"
fi
if ! grep -q "test result: ok. 3 passed" "$log"; then
    echo "ERROR: expected 3 passed tests in sidecar_lifecycle" >&2
    exit 1
fi
