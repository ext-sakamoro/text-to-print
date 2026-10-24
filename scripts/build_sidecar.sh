#!/usr/bin/env bash
# Build alice-llm-server binary for text-to-print packaging bundle
#
# Prerequisites:
#   - Sibling checkout at ../ALICE-LLM (or $ALICE_LLM_DIR)
#   - Rust toolchain with the desired target installed
#
# Usage:
#   scripts/build_sidecar.sh
#   scripts/build_sidecar.sh --target aarch64-apple-darwin
#   ALICE_LLM_DIR=/path/to/ALICE-LLM OUT_DIR=/some/dir scripts/build_sidecar.sh
#
# CI (.github/workflows/release.yml) invokes this before staging the
# desktop binary so cargo-wix / cargo-deb / linuxdeploy pick up both
#
# Output:
#   Copies alice-llm-server (or .exe on Windows) into OUT_DIR
#   Default OUT_DIR = target/release

set -euo pipefail

ALICE_LLM_DIR="${ALICE_LLM_DIR:-../ALICE-LLM}"
OUT_DIR="${OUT_DIR:-target/release}"
TARGET=""

# Parse --target flag (positional or named)
while [ $# -gt 0 ]; do
    case "$1" in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --target=*)
            TARGET="${1#--target=}"
            shift
            ;;
        *)
            echo "usage: $0 [--target <triple>]" >&2
            exit 2
            ;;
    esac
done

if [ ! -d "$ALICE_LLM_DIR" ]; then
    echo "ERROR: ALICE-LLM checkout not found at $ALICE_LLM_DIR" >&2
    echo "  clone: git clone https://github.com/Project-ALICE/ALICE-LLM \"$ALICE_LLM_DIR\"" >&2
    exit 1
fi

echo "==> Building alice-llm-server (features=server)"
if [ -n "$TARGET" ]; then
    (cd "$ALICE_LLM_DIR" && cargo build --release --features server --bin alice-llm-server --target "$TARGET")
    SRC="$ALICE_LLM_DIR/target/$TARGET/release/alice-llm-server"
else
    (cd "$ALICE_LLM_DIR" && cargo build --release --features server --bin alice-llm-server)
    SRC="$ALICE_LLM_DIR/target/release/alice-llm-server"
fi

# Windows binary ends in .exe fall through if the non-suffixed version is missing
if [ ! -f "$SRC" ] && [ -f "$SRC.exe" ]; then
    SRC="$SRC.exe"
fi

if [ ! -f "$SRC" ]; then
    echo "ERROR: expected binary not found at $SRC" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"
cp "$SRC" "$OUT_DIR/"
echo "==> Copied $(basename "$SRC") -> $OUT_DIR/"
ls -la "$OUT_DIR/$(basename "$SRC")"
