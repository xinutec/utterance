#!/usr/bin/env bash
# What the tests actually reach, both halves.
#
#   nix develop --command scripts/coverage.sh
#
# NOT part of scripts/verify.sh, and deliberately not gated on a threshold.
# Coverage says which lines a test executed, not whether it would notice them
# being wrong — a suite that calls everything and asserts nothing scores well.
# Treat a number here as a place to go looking, and settle the question by
# breaking the code on purpose and checking that something fails.
#
# Two numbers are reported for Rust because the honest one depends on what you
# count. src/bin/* are research tools run by hand — `authority` measures what
# each knob does, `beating` measures roughness, `dwell` chord ring time,
# `streams` correlations between the analysis streams — and main.rs is wiring.
# None are shipped logic and none are tested; left in the denominator they hide
# how well the parts that ARE shipped are covered.
set -euo pipefail
cd "$(dirname "$0")/.."

for tool in cargo llvm-profdata pnpm; do
  command -v "$tool" >/dev/null || {
    echo "coverage: $tool not on PATH — run inside \`nix develop\`." >&2
    exit 1
  }
done

# Its own target dir, for the same reason clippy has one: the coverage build
# carries instrumentation, so sharing a directory with a normal build makes each
# evict the other and recompile the world every time.
export CARGO_TARGET_DIR="${CARGO_COVERAGE_TARGET_DIR:-$HOME/.cache/cargo/llvmcov-target}"
# cargo-llvm-cov looks for a rustup component that a nix toolchain does not
# have; point it at the LLVM in the dev shell instead.
export LLVM_COV="$(command -v llvm-cov)"
export LLVM_PROFDATA="$(command -v llvm-profdata)"

echo "== rust =="
report="$(mktemp)"
trap 'rm -f "$report"' EXIT
cargo llvm-cov --workspace --summary-only 2>/dev/null | tee "$report" | tail -1

awk '
  /^Filename.*Regions/ { rows = 1; next }
  rows && /^(src|utterance)/ {
    regions += $2; missed += $3; lines += $8; missed_lines += $9
    if ($1 ~ /^src\/bin\/|^src\/main\.rs$/) {
      tool_regions += $2; tool_missed += $3; tool_lines += $8; tool_missed_lines += $9
    }
  }
  END {
    printf "  all           %6.2f%% of regions, %6.2f%% of lines\n",
      100 * (regions - missed) / regions, 100 * (lines - missed_lines) / lines
    printf "  shipped only  %6.2f%% of regions, %6.2f%% of lines  (excludes src/bin/* and main.rs)\n",
      100 * ((regions - tool_regions) - (missed - tool_missed)) / (regions - tool_regions),
      100 * ((lines - tool_lines) - (missed_lines - tool_missed_lines)) / (lines - tool_lines)
  }
' "$report"

echo
echo "== frontend =="
# --coverage-include is load-bearing: v8 reports only files the specs imported,
# so without it a file with no test at all is absent from the report rather than
# counted as zero, and the total flatters itself by the size of what it omits.
# Measured 2026-08-03: 85.91% reported, 22.12% once every app file was counted.
#
# Components read 0% here and are not untested — the 21 Playwright specs in
# frontend/e2e exercise them in a real browser, which this instrument cannot
# see. It measures the vitest run alone.
( cd frontend && pnpm exec ng test --watch=false --coverage \
    --coverage-reporters text-summary \
    --coverage-include 'src/app/**/*.ts' \
    --coverage-exclude 'src/app/generated/**' \
    --coverage-exclude '**/*.spec.ts' 2>&1 | sed -n '/Coverage report/,$p' )
