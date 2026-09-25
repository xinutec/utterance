{-
utterance/gate.dhall — this repository's commit gate.

Rendered to the committed `gate.json`; `the table matches its Dhall` re-renders
and diffs it, so running the gate needs no `dhall`. The shared vocabulary —
`inDevShell`, the clippy target directory, the `ng-build`, `dev-lint` and
`check-table` rows — comes from dev-lint's schema as `G.` values.

The runner takes one lock per worktree and refuses rather than queues, because
two runs share the working tree: the generated-types row regenerates into
`frontend/src/app/generated` while comparing it, and a second run would report
drift that does not exist.

Exactly one row writes `dist/`, and `ui-check` reads what it wrote, so a build
failure is reported as a build failure rather than as a harness one.

`pnpm install` runs unconditionally, and not as a speed trade. Deciding *whether*
to install is the same question as installing — does node_modules match the
lockfile — and pnpm answers it from its own install record while a shell test can
only guess: a half-written tree can have an executable `.bin/eslint` and still
be missing a package lint needs. It costs well under a second when there is
nothing to do.
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
      , {-  Rustdoc's link resolution. Nothing else in this table sees it: the
            compiler does not read doc comments, and clippy does not follow the
            links inside them, so a `[`Thing`]` naming something that moved or was
            never public renders as literal text and reads as prose.
        -}
        G.cargoDoc
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
        , env = G.nonInteractive
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend lint"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "lint" ]
        , env = G.nonInteractive
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend typecheck (e2e)"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "typecheck:e2e" ]
        , env = G.nonInteractive
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend unit tests"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "test" ]
        , env = G.nonInteractive # toMap { NG_BUILD_MAX_WORKERS = "1" }
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
        , env = G.nonInteractive
        , timeout_s = 1800
        }
      , {-  The L2 phone-width layout harness, serving the dist the build row
            wrote — so it must come after that row.
        -}
        G.Check::{
        , name = "frontend ui-check (phone-width layout harness)"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "ui-check" ]
        , {-  Playwright DELETES this at the start of every run, so the run made
              to investigate a failure is the run that erases it — and no option
              turns that off (`preserveOutput` is about PASSING tests). Declaring
              it here makes the gate copy it aside when this check fails.
          -}
          artifacts = [ "test-results" ]
        , env = G.nonInteractive
        , timeout_s = 1800
        }
      , {-  A green gate has to mean the package this repo PUBLISHES still builds.

            ⚠ This is NOT the same work as the `tests` row above, though it looks
            like it. That row runs cargo in the dev shell against the working
            tree; this one builds the derivation, which resolves dependencies
            from the committed lockfile and compiles inside /nix/store, running
            the tests there (`doCheck` in flake.nix).
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
