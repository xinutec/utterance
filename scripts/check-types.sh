#!/usr/bin/env bash
# Drift gate: regenerate the TS types and fail if the committed output changed.
# Catches a Rust type edit that was not regenerated and committed. Runs inside
# scripts/verify.sh, i.e. the pre-commit gate.
set -euo pipefail
cd "$(dirname "$0")/.."

# Snapshot first and compare the regenerated output against THAT, not against
# the git index. `git diff` answers "are these staged?", a different question
# that reports false drift in the normal edit → verify → commit order.
before="$(mktemp -d)"
trap 'rm -rf "$before"' EXIT
cp -R frontend/src/app/generated/. "$before"/

scripts/gen-types.sh >/dev/null

if ! diff -r -q "$before" frontend/src/app/generated >/dev/null 2>&1; then
  echo "gen-types drift: the Rust types changed but frontend/src/app/generated/" >&2
  echo "was not regenerated. Run 'nix develop --command scripts/gen-types.sh' and commit." >&2
  diff -r -q "$before" frontend/src/app/generated >&2 || true
  exit 1
fi
echo "types in sync."
