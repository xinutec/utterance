#!/usr/bin/env bash
# utterance verify — rust workspace (fmt + clippy + tests) + generated-type drift +
# angular frontend (lint + unit tests + layout harness) + shared dev-lint rules.
# This is the whole health gate; scripts/githooks/pre-commit runs it.
set -euo pipefail
cd "$(dirname "$0")/.."

# One run at a time, and say so rather than queueing.
#
# Two of these overlapping do not merely waste a CPU: they share the working
# tree. `scripts/check-types.sh` regenerates into `frontend/src/app/generated`
# while the other run is comparing that directory against a snapshot, so the
# second reports drift that does not exist — and worse, leaves the loser's temp
# directory behind inside `generated/`, which the *next* run then reports as
# drift too. That happened, and cost more time to diagnose than every run it
# would ever block.
#
# Refused rather than queued. A second run started by hand is nearly always
# someone who forgot the first, and telling them beats silently making them wait
# for two full gates.
# `mkdir` rather than `flock`, which macOS does not ship. Creating a directory
# is atomic everywhere, and the pid inside it is what lets a lock left behind by
# a killed run be told from a live one.
lock="${TMPDIR:-/tmp}/utterance-verify.lock"
if ! mkdir "$lock" 2>/dev/null; then
  owner="$(cat "$lock/pid" 2>/dev/null || true)"
  if [ -n "$owner" ] && kill -0 "$owner" 2>/dev/null; then
    echo "verify is already running as pid $owner." >&2
    echo "Wait for it or kill it — two at once corrupt each other's generated types." >&2
    exit 2
  fi
  echo "clearing a stale lock left by pid ${owner:-unknown}" >&2
  rm -rf "$lock"
  mkdir "$lock"
fi
printf '%s\n' "$$" >"$lock/pid"
trap 'rm -rf "$lock"' EXIT

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
  # --frozen-lockfile is pnpm ci: install exactly pnpm-lock.yaml, or fail. The
  # guard is not just a speed-up — a node_modules left behind by npm still has a
  # working .bin, so verify would pass against packages the lockfile no longer
  # describes.
  if [ ! -x frontend/node_modules/.bin/eslint ] || [ frontend/pnpm-lock.yaml -nt frontend/node_modules ]; then
    ( cd frontend && pnpm install --frozen-lockfile )
  fi
  ( cd frontend && pnpm run lint && pnpm test && pnpm run ui-check )
'

dev_lint_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/dev-lint"
[ -d "$dev_lint_dir" ] || dev_lint_dir="$HOME/Code/dev-lint"
[ -d "$dev_lint_dir" ] || dev_lint_dir="$HOME/code/dev-lint"
nix run "$dev_lint_dir" -- . # dev-lint
