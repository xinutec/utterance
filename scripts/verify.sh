#!/usr/bin/env bash
# music verify — rust workspace (fmt + clippy + tests) + generated-type drift +
# angular frontend (lint + unit tests + layout harness) + shared dev-lint rules.
# This is the whole health gate; scripts/githooks/pre-commit runs it.
set -euo pipefail
cd "$(dirname "$0")/.."

nix develop -c bash -c '
  set -euo pipefail
  cargo fmt --all --check

  # Clippy gets its own target dir: clippy-driver and rustc fingerprint the
  # workspace differently and evict each other in a shared dir, forcing a full
  # recompile every time. A dedicated dir keeps both caches warm.
  CARGO_TARGET_DIR="${CARGO_CLIPPY_TARGET_DIR:-$HOME/.cache/cargo/clippy-target}" \
    cargo clippy --workspace --all-targets -- -D warnings

  # The ts feature (which pulls ts-rs) stays off here on purpose — normal builds
  # must not carry it. scripts/check-types.sh below turns it on for generation.
  cargo test --workspace

  # Regenerate the frontend TS from the Rust types and fail on drift.
  scripts/check-types.sh

  # ng build (via ui-check) intermittently aborts at libuv/kqueue teardown on
  # macOS; NG_BUILD_MAX_WORKERS=1 lowers the rate. Harmless on Linux/CI.
  export NG_BUILD_MAX_WORKERS=1
  if [ ! -x frontend/node_modules/.bin/eslint ] || [ frontend/package-lock.json -nt frontend/node_modules ]; then
    ( cd frontend && npm ci )
  fi
  ( cd frontend && npm run lint && npm test && npm run ui-check )
'

dev_lint_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/dev-lint"
[ -d "$dev_lint_dir" ] || dev_lint_dir="$HOME/Code/dev-lint"
[ -d "$dev_lint_dir" ] || dev_lint_dir="$HOME/code/dev-lint"
nix run "$dev_lint_dir" -- . # dev-lint
