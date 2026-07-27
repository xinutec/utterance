#!/usr/bin/env bash
# Run both halves locally: the API on :8181 and ng serve on :4200 with /api
# proxied to it. Ctrl-C stops both.
#
# The backend is started API-only (no STATIC_DIR) so the page you look at is the
# one ng serve rebuilds on save, not a stale bundle in dist/.
set -euo pipefail
cd "$(dirname "$0")/.."

nix develop -c bash -c '
  set -euo pipefail

  if [ ! -d frontend/node_modules ]; then
    ( cd frontend && npm ci )
  fi

  cargo build

  cargo run &
  backend=$!
  # Take the whole process group down on exit, or ng serve outlives the script
  # and holds :4200 against the next run.
  trap "kill $backend 2>/dev/null || true" EXIT INT TERM

  ( cd frontend && npm start )
'
