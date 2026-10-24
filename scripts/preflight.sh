#!/usr/bin/env bash
# scripts/preflight.sh — local reproduction of the CI gates before `git push`.
# Every command is the one CI runs. A step this file does not cover is a step
# that can only fail remotely — when a workflow step is added, regenerate or
# add it here in the same commit. Run `--quick` before every push.
#
# usage: scripts/preflight.sh [--quick]   (--quick skips the test / bench suites)
set -euo pipefail
cd "$(dirname "$0")/.."

quick=0
[[ "${1:-}" == "--quick" ]] && quick=1

step() { printf '\n\033[1;34m== %s\033[0m\n' "$*"; }
# `cargo clippy` reuses fresh `cargo check` artifacts and then lints nothing;
# touching the crate roots invalidates only this repo's fingerprints.
relint() { git ls-files | grep -E '(^|/)src/(lib|main)\.rs$' | xargs -r touch; }
need() { command -v "$1" >/dev/null 2>&1 || { echo "missing tool: $1 ($2)" >&2; exit 1; }; }
has_toolchain() { rustup toolchain list | grep -q "^$1"; }

# Steps CI runs that this file cannot reproduce locally (they can only fail remotely):
#   - ci.yml:audit:Install cargo-audit (needs network / runner-only)
#   - ci.yml:clippy-test-doc:Install libdbus-1-dev (Linux only) (no cargo / grep)
#   - security-audit.yml:deny:Install cargo-deny (needs network / runner-only)
#   - security-audit.yml:coverage (the coverage step is continue-on-error: informational in CI)
#   - semver-checks.yml:semver-checks (dormant: started by hand only, informational)
#   - fuzz.yml:fuzz (the replay of the corpus needs the runner; a crash fails the job)

need actionlint "brew install actionlint"
need cargo-audit "cargo install cargo-audit --locked"
need cargo-deny "cargo install cargo-deny --locked"
need cargo-machete "cargo install cargo-machete --locked"

step "ci.yml / clippy-test-doc: cargo fmt --check"
python3 scripts/fmt_scope_check.py --self-test
python3 scripts/fmt_scope_check.py

step "ci.yml / clippy-test-doc: cargo clippy (workspace, all targets)"
relint
( export CARGO_TERM_COLOR="always"; cargo clippy --locked --workspace --all-targets -- -D warnings )

step "ci.yml / clippy-test-doc: cargo doc (no deps)"
( export CARGO_TERM_COLOR="always" RUSTDOCFLAGS="-D warnings"; cargo doc --workspace --no-deps )

step "ci.yml / wiring-guard: oracle + 新規の未配線 / 理由の無い dead_code が無い"
python3 scripts/test_wiring_guard.py
python3 scripts/test_workflow_timeouts.py
python3 scripts/workflow_timeouts.py
python3 scripts/wiring_guard.py

step "ci.yml / wiring-guard: vocab-lint (許可リスト外の token が無い)"
python3 scripts/vocab_lint.py --self-test
python3 scripts/vocab_lint.py

step "public-text.yml / public-text: Public text (ALICE-Law checker at the pin; tree, then the commits)"
python3 scripts/test_public_text.py
scripts/public_text.sh

step "ci.yml / wiring-guard: image-metadata (追跡している PNG / JPEG に metadata が無い)"
python3 scripts/test_image_metadata_check.py
python3 scripts/image_metadata_check.py

step "ci.yml / clippy-test-doc: sibling pins (固定 commit と cargo の解決の突合)"
python3 scripts/ci_sibling_checkout_check.py --self-test
python3 scripts/ci_sibling_checkout_check.py

step "ci.yml / clippy-test-doc: no peer-to-peer layer (libp2p が無い)"
python3 scripts/no_p2p_guard.py --self-test
python3 scripts/no_p2p_guard.py

step "ci.yml / actionlint: actionlint"
actionlint .github/workflows/*.yml

step "ci.yml / actionlint: changed paths tests + trigger lint"
python3 scripts/test_changed_paths.py
python3 scripts/changed_paths.py sync

step "ci.yml / check-wasm-worker: cargo check --target wasm32-unknown-unknown"
rustup target list --installed | grep -q '^wasm32-unknown-unknown$' || rustup target add wasm32-unknown-unknown
(cd "crates/worker" && ( export CARGO_TERM_COLOR="always"; cargo check --locked --target wasm32-unknown-unknown --release ))

step "security-audit.yml / deny: Licence exceptions equal the declarations (tests, then the check)"
python3 scripts/test_deny_exception_guard.py
python3 scripts/deny_exception_guard.py

step "security-audit.yml / deny: Run cargo deny check all"
( export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"; cargo deny --all-features check all )

step "security-audit.yml / unused-deps: Run cargo machete"
( export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"; cargo machete )

step "security-audit.yml / stub-guard: Detect todo! / unimplemented! / panic!(STUB) in src/**"
(
  export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"
  set -eo pipefail
  hits=$(grep -rnE 'todo!\(|unimplemented!\(|panic!\([^)]*STUB' \
    crates/ --include="*.rs" \
    --exclude-dir=bin \
    --exclude-dir=target \
    || true)
  if [ -n "$hits" ]; then
    echo "❌ Stub / unimplemented / STUB panic detected in production path:"
    echo "$hits"
    exit 1
  fi
  echo "✓ No todo! / unimplemented! / panic!(STUB) in crates/"
)

step "security-audit.yml / stub-guard: Detect dbg!() residual"
(
  export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"
  set -eo pipefail
  hits=$(grep -rn 'dbg!(' crates/ --include="*.rs" \
    --exclude-dir=target || true)
  if [ -n "$hits" ]; then
    echo "❌ dbg!() macro left in crates/:"
    echo "$hits"
    exit 1
  fi
  echo "✓ No dbg!() in crates/"
)

step "security-audit.yml / stub-guard: Detect TODO / FIXME / XXX / HACK (informational)"
(
  export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"
  set -eo pipefail
  hits=$(grep -rnE 'TODO|FIXME|XXX|HACK' crates/ --include="*.rs" \
    --exclude-dir=target || true)
  if [ -n "$hits" ]; then
    echo "::warning::TODO/FIXME/XXX/HACK found (informational):"
    echo "$hits" | head -50
  else
    echo "✓ No TODO/FIXME/XXX/HACK in crates/"
  fi
)

step "fuzz.yml / build every fuzz target (nightly; the replay needs the runner)"
if has_toolchain nightly && cargo +nightly fuzz --version >/dev/null 2>&1; then
  (cd fuzz && cargo +nightly fuzz build)
else
  echo "skip: nightly / cargo-fuzz not installed" >&2
fi

if [[ $quick -eq 1 ]]; then
  echo; echo "preflight --quick OK (test / bench suites skipped)"; exit 0
fi

step "ci.yml / audit: cargo audit (honours .cargo/audit.toml ignore list)"
( export CARGO_TERM_COLOR="always"; cargo audit )

step "ci.yml / clippy-test-doc: cargo test (workspace, all targets)"
( export CARGO_TERM_COLOR="always"; cargo test --locked --workspace --all-targets )

step "ci.yml / check-wasm-worker: cargo test (host)"
worker_log="$(mktemp)"
(cd "crates/worker" && ( set -o pipefail; cargo test --locked 2>&1 | tee "$worker_log" ))
grep -q "signature_scope_tests::a_changed_nickname_is_refused ... ok" "$worker_log"
grep -q "signature_scope_tests::bytes_moved_between_the_source_and_the_date_are_refused ... ok" "$worker_log"
rm -f "$worker_log"

step "ci.yml / clippy-test-doc: sidecar lifecycle (real alice-llm-server)"
( export CARGO_TERM_COLOR="always"; scripts/sidecar_lifecycle_test.sh )

echo; echo "preflight OK"
