#!/usr/bin/env bash
# Generate the frontend TS interfaces from the Rust types via ts-rs, so the
# backend↔frontend wire shapes are consistent by construction, not transcribed.
#
# Run inside the dev shell (cargo on PATH):
#   nix develop --command scripts/gen-types.sh
#
# Output lands in frontend/src/app/generated/ (committed; re-exported through
# frontend/src/app/models.ts). scripts/check-types.sh re-runs this and fails if
# the committed output has drifted.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="frontend/src/app/generated"

# Generate into a scratch dir FIRST and only replace the committed output once it
# has actually worked. A generator that fails must leave the previous output
# exactly where it was — otherwise one compile error in the test tree deletes
# every committed type and leaves the frontend unbuildable for a reason nothing
# reported.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ts-rs emits one file per #[ts(export)] type; its export tests are named
# export_bindings_*, so this filter runs generation only. TS_RS_EXPORT_DIR is
# pinned in .cargo/config.toml and overridden here so a failed run cannot touch
# the committed types. --features ts turns ts-rs on (off in normal builds);
# --workspace so utterance-analysis's voiceprint types export alongside the
# server's wire types.
if ! TS_RS_EXPORT_DIR="$TMP" cargo test --workspace --features ts export_bindings >"$TMP/cargo.log" 2>&1; then
  echo "gen-types: generation failed — committed types left untouched." >&2
  grep -E '^(error|warning: unused)|^ *-->' "$TMP/cargo.log" >&2 || tail -30 "$TMP/cargo.log" >&2
  exit 1
fi

count="$(find "$TMP" -name '*.ts' | wc -l | tr -d ' ')"
if [ "$count" -eq 0 ]; then
  echo "gen-types: generation produced no types — committed types left untouched." >&2
  exit 1
fi

rm -rf "$OUT"
mkdir -p "$(dirname "$OUT")"
cp -R "$TMP" "$OUT"
rm -f "$OUT/cargo.log"
echo "generated $count type(s) -> $OUT"
