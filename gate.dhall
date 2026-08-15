{-
utterance/gate.dhall — this repository's commit gate.

Was `scripts/verify.sh`. Three of its parts moved rather than disappeared.

**The lock is now the runner's.** Thirty of the script's seventy-five lines were
a `mkdir`-based mutex with a pid file and stale-lock detection, because two runs
share the working tree: the generated-types row regenerates into
`frontend/src/app/generated` while the other run compares that directory to a
snapshot, so the second reports drift that does not exist and leaves the loser's
temp directory inside `generated/` for the next run to report as drift too. That
reasoning is not utterance's — any table with a regeneration step, a `dist/`, or
a shared `CARGO_TARGET_DIR` races itself the same way — so `gate` takes one lock
per worktree for every repository now, still refusing rather than queueing. It
uses an advisory lock on an open file, which the kernel releases when the process
dies, so the stale-lock branch this script needed is simply gone.

**The build is a row of its own.** `pnpm run ui-check` was `ng build &&
playwright test`, so the only build in the gate was inside the layout-harness
step and a build failure was reported as a harness failure. It is now
`playwright test`, reading what the build row wrote. That is also what lets
`ng-build` mean something: exactly one thing in the gate writes `dist/`, and it
has to prove it did — index.html present, non-empty, rewritten by this run, and
every script it names parseable as an ES module. The script's own note said the
kqueue teardown abort was "harmless"; it no longer has to be remembered, because
nothing now judges the build by its exit status.

**The `&&` chain is gone.** `pnpm run lint && pnpm run typecheck:e2e && pnpm test
&& pnpm run ui-check` reported one name when four things could be wrong.

Kept verbatim, because it is the best-argued version of this in the fleet: the
`pnpm install` is unconditional, and not as a speed trade. Deciding *whether* to
install is the same question as installing — does node_modules match the
lockfile — and pnpm answers it from its own install record while a shell test can
only guess. The guard this replaced looked for an executable `.bin/eslint`, which
a half-written tree has: a node_modules missing eslint-visitor-keys passed the
check and then failed lint with a module-resolution error naming a package nobody
had touched. 460 ms when there is nothing to do.

The generated `gate.json` is committed; `the table matches its Dhall` re-renders
and diffs it, so running the gate needs no `dhall`.

**The vocabulary moved into the schema.** `inDevShell`, the clippy target
directory, the Angular worker cap, and the `ng-build` / `dev-lint` /
`check-table` rows were spelled out here and in a dozen other tables
identically — the duplication the shared tools were built to remove, recreated
one level up. They are `G.` values now. Two consequences the rendered JSON
shows: every dev-shell row gains `--no-warn-dirty`, because a gate that prints
"Git tree is dirty" on every row of every run has trained everyone to ignore a
warning; and dev-lint is pinned to its committed HEAD rather than run out of its
worktree, which is what stops a neighbour's half-finished edit failing this gate
for a reason no commit anywhere explains.

-}

let G = ../dev-lint/gate/schema.dhall

in  { name = "utterance"
    , checks =
      [ G.Check::{
        , name = "formatting"
        , argv = G.inDevShell [ "cargo", "fmt", "--all", "--check" ]
        , timeout_s = 180
        }
      , {-  Clippy gets its own target directory: clippy-driver and rustc
            fingerprint the workspace differently and evict each other in a
            shared one, forcing a full recompile every time.

            The script read this from `$CARGO_CLIPPY_TARGET_DIR` with the path
            below as the default. A table's `env` is data, not shell, so there is
            no expansion — and the override had no other caller, so the default
            is simply the value now.
        -}
        G.Check::{
        , name = "clippy"
        , argv =
            G.inDevShell
              [ "cargo"
              , "clippy"
              , "--workspace"
              , "--all-targets"
              , "--"
              , "-D"
              , "warnings"
              ]
        , env =
            G.clippyTarget
        , timeout_s = 1800
        }
      , {-  The `ts` feature (which pulls ts-rs) stays off here on purpose —
            normal builds must not carry it. `scripts/gen-types.sh` below turns it
            on for generation.
        -}
        G.Check::{
        , name = "tests"
        , argv = G.inDevShell [ "cargo", "test", "--workspace" ]
        , timeout_s = 1800
        }
      , {-  Regenerate the frontend TS from the Rust types and fail on drift.
            This is the row the worktree lock exists for: it writes into
            `frontend/src/app/generated` while comparing it.
        -}
        G.Check::{
        , name = "generated types are current"
        , argv = G.inDevShell [ "scripts/gen-types.sh", "--check" ]
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend deps match the lockfile"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "install", "--frozen-lockfile" ]
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend lint"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "lint" ]
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend typecheck (e2e)"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "typecheck:e2e" ]
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend unit tests"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "test" ]
        , env = toMap { NG_BUILD_MAX_WORKERS = "1" }
        , timeout_s = 1800
        }
      , {-  `../../dev-lint`, not `../dev-lint`: cwd is `utterance/frontend`.
        -}
        G.Check::{
        , name = "frontend build"
        , cwd = "frontend"
        , argv =
            G.ngBuild
              "../../"
              [ "dist/utterance-web/browser" ]
              [ "pnpm", "exec", "ng", "build" ]
        , timeout_s = 1800
        }
      , {-  The L2 phone-width layout harness, serving the dist the build row
            wrote. Placement is load-bearing here rather than presentation:
            `ui-check` used to build first, and no longer does.
        -}
        G.Check::{
        , name = "frontend ui-check (phone-width layout harness)"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "ui-check" ]
        , timeout_s = 1800
        }
      , {-  A green gate has to mean the package this repo PUBLISHES still builds.

            ⚠ This is NOT the same work as the `tests` row above, though it looks
            like it. That row runs cargo in the dev shell against the working
            tree; this one builds the derivation, which resolves dependencies
            from the committed lockfile and compiles inside /nix/store. The
            package's own comment says the build runs the tests deliberately, for
            the external-volume reason — this row is what makes that promise
            checkable rather than aspirational.
        -}
        G.Check::{
        , name = "the package builds (what this repo publishes)"
        , argv = [ "nix", "build", "--no-warn-dirty", "--no-link", ".#default" ]
        , timeout_s = 1800
        }
      , G.checkTable "../dev-lint"
      , G.devLint "../"
      ]
    }
