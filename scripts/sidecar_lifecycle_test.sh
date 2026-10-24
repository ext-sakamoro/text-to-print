#!/usr/bin/env bash
# 同梱の sidecar の寿命と待ち受けを実 binary で確かめる (ci.yml と preflight の共通)
#
#   1. ../ALICE-LLM (sibling の固定 commit) から alice-llm-server を build
#   2. crates/llm/tests/sidecar_lifecycle.rs の ignore 付き試験を走らせる
#      (app 役の process を SIGTERM / SIGKILL で止め、sidecar の process と port が消えること)
#   3. 走った試験の数を確かめる (3 本、helper 1 + 試験 2) 0 本や feature 違いの空振りを失敗にする
set -euo pipefail

ALICE_LLM_DIR="${ALICE_LLM_DIR:-../ALICE-LLM}"
if [ ! -d "$ALICE_LLM_DIR" ]; then
    echo "ERROR: ALICE-LLM checkout not found at $ALICE_LLM_DIR" >&2
    exit 1
fi
(cd "$ALICE_LLM_DIR" && cargo build --locked --features server --bin alice-llm-server)
ALICE_LLM_SERVER_BIN="$(cd "$ALICE_LLM_DIR" && pwd)/target/debug/alice-llm-server"
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
