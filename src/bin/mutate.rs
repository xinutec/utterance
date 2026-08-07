//! Would the tests notice? Break the code on purpose and find out.
//!
//!     nix develop --command cargo run --bin mutate
//!     nix develop --command cargo run --bin mutate -- separation   # one subject
//!
//! Sibling of `scripts/coverage.sh`, and the answer to the question that script
//! cannot answer. Coverage says which lines a test executed; it says nothing
//! about whether the test would have complained had those lines been wrong. A
//! suite that calls everything and asserts nothing scores well. Every mutant
//! below is a line of shipped logic changed to something a careless edit could
//! plausibly produce, run against the tests, and put back.
//!
//! NOT in the gate and not thresholded, for the same reason coverage is not:
//! it takes tens of minutes, it rebuilds and re-runs a test suite once per
//! mutant, and the useful output is a name to go and look at rather than a
//! number to keep above a line.
//!
//! **Each mutant is judged by its OWN crate's tests**, not the whole workspace.
//! Partly arithmetic — measured 2026-08-07, `cargo test --workspace` is over ten
//! minutes here against 220s for analysis, 92s for mapping and 46s for
//! realisation, so twelve mutants is twenty minutes rather than two hours, and a
//! tool nobody waits for answers nothing. Mostly, though, it is the sharper
//! question: these are designated pure cores, and "utterance-mapping's own suite
//! notices when utterance-mapping is wrong" is the claim worth holding. A mutant
//! that survives its crate and dies in `tests/api.rs` is being caught by
//! accident, three layers away, and that is worth knowing separately — so a
//! survivor is worth re-running with `--workspace` by hand before concluding
//! nothing sees it.
//!
//! **Three traps, all of them hit the first time this was done by hand.**
//!
//! 1. *A pattern that does not apply reports nothing wrong.* The suite passes,
//!    the mutant is recorded as survived-or-killed on a file that was never
//!    edited, and the run looks like evidence. So a pattern must match EXACTLY
//!    ONCE or the entry is reported broken and no test is run for it.
//! 2. *`cargo fmt` silently breaks patterns.* Rewrapping one line is enough for
//!    a `from` string to stop matching, and the failure mode is trap 1. Nothing
//!    here reformats, and trap 1's check is what catches the day someone else
//!    does.
//! 3. *An uncompilable mutant looks exactly like a killed one.* Both are a
//!    non-zero cargo exit. A mutant that does not build tested nothing, so the
//!    compiler's own verdict is read out of the output and reported separately.
//!
//! Equivalent mutants — a change that genuinely cannot alter behaviour — are
//! expected, and are a fact about the code rather than a weakness in the tests.
//! They are declared as [`Expect::Survives`] with the argument for why, and a
//! declared survivor that gets killed is as much a finding as the reverse.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the suite is supposed to do with a mutant.
enum Expect {
    /// Something must fail. The normal case.
    Killed,
    /// Nothing can fail, because the change cannot alter behaviour. The string
    /// is the argument for that, and it has to be an argument about the code —
    /// "no test covers it" is a finding, not an equivalence.
    ///
    /// Unused as written, because which mutants are equivalent is what a run
    /// finds out rather than something to assert in advance. `expect` and not
    /// `allow` so that the day one is added, this attribute becomes unfulfilled
    /// and clippy says to delete it.
    #[expect(dead_code, reason = "no mutant has been argued equivalent yet")]
    Survives(&'static str),
}

struct Mutant {
    /// Repository-relative.
    file: &'static str,
    /// Must occur exactly once in `file`.
    from: &'static str,
    to: &'static str,
    /// The claim that stops being true. Also the filter key.
    claim: &'static str,
    expect: Expect,
}

/// Each entry names a claim some test is supposed to be making. Spread across
/// the three pure crates rather than concentrated, because the question is
/// whether the SUITE notices, and the suites differ in how they were written:
/// the mapping tests assert on structure, the realisation tests on rendered
/// samples, and the analysis tests on real recordings under `tests/fixtures`.
const MUTANTS: &[Mutant] = &[
    // ---- utterance-mapping ----
    Mutant {
        file: "utterance-mapping/src/tonnetz.rs",
        from: "placed + 1200.0 * ((floor - placed) / 1200.0).ceil()",
        to: "placed + 1200.0 * ((floor - placed) / 1200.0).floor()",
        claim: "separation: two voices never land within MIN_SEPARATION_CENTS",
        expect: Expect::Killed,
    },
    Mutant {
        file: "utterance-mapping/src/tonnetz.rs",
        from: "let gap = (CLOSE_POSITION_CENTS * params.spacing as f32).min(MAX_SPAN_CENTS / top);",
        to: "let gap = (CLOSE_POSITION_CENTS * params.spacing as f32).max(MAX_SPAN_CENTS / top);",
        claim: "span: a chord stays inside MAX_SPAN_CENTS however wide the spacing knob",
        expect: Expect::Killed,
    },
    Mutant {
        file: "utterance-mapping/src/lattice.rs",
        from: "if c >= 1200.0 { 0.0 } else { c }",
        to: "if c > 1200.0 { 0.0 } else { c }",
        claim: "wrap: a pitch class of exactly an octave is zero, not an octave",
        expect: Expect::Killed,
    },
    Mutant {
        file: "utterance-mapping/src/lattice.rs",
        from: "let interval = wrapped.min(1200.0 - wrapped);",
        to: "let interval = wrapped.max(1200.0 - wrapped);",
        claim: "inversion: an interval is measured the short way round",
        expect: Expect::Killed,
    },
    Mutant {
        file: "utterance-mapping/src/params.rs",
        from: "value.round().max(0.0) as usize",
        to: "value.max(0.0) as usize",
        claim: "knobs: a count knob rounds rather than truncating",
        expect: Expect::Killed,
    },
    // ---- utterance-realisation ----
    Mutant {
        file: "utterance-realisation/src/synth.rs",
        from: "let end = (start + samples).min(out.len());",
        to: "let end = start + samples;",
        claim: "bounds: an event running past the end of the buffer is clipped",
        expect: Expect::Killed,
    },
    Mutant {
        file: "utterance-realisation/src/synth.rs",
        from: "let length = (score.duration_s.max(0.0) * RENDER_RATE as f32).ceil() as usize;",
        to: "let length = (score.duration_s.max(0.0) * RENDER_RATE as f32) as usize;",
        claim: "length: the buffer is long enough for the last partial sample",
        expect: Expect::Killed,
    },
    Mutant {
        file: "utterance-realisation/src/synth.rs",
        from: "if start >= out.len() || event.hz <= 0.0 || event.duration_s <= 0.0 {",
        to: "if start >= out.len() || event.duration_s <= 0.0 {",
        claim: "silence: an event at zero Hz renders nothing",
        expect: Expect::Killed,
    },
    Mutant {
        file: "utterance-realisation/src/synth.rs",
        from: "if partial_hz >= nyquist {",
        to: "if partial_hz > nyquist {",
        claim: "aliasing: a partial exactly at Nyquist is dropped",
        expect: Expect::Killed,
    },
    // ---- utterance-analysis ----
    Mutant {
        file: "utterance-analysis/src/texture.rs",
        from: "let lowest = ((NOISE_BAND_LOW_HZ / bin_hz).ceil() as usize).min(bins - 1);",
        to: "let lowest = ((NOISE_BAND_LOW_HZ / bin_hz).floor() as usize).min(bins - 1);",
        claim: "bands: the noise band starts at or above its stated low edge",
        expect: Expect::Killed,
    },
    Mutant {
        file: "utterance-analysis/src/texture.rs",
        from: "let octave_mean = octaves.iter().sum::<f32>() / octaves.len().max(1) as f32;",
        to: "let octave_mean = octaves.iter().sum::<f32>() / octaves.len() as f32;",
        claim: "empty: an empty octave list divides by one rather than by zero",
        expect: Expect::Killed,
    },
    Mutant {
        file: "utterance-analysis/src/speaker.rs",
        from: "if low_hz <= 0.0 || high_hz <= low_hz {",
        to: "if low_hz <= 0.0 {",
        claim: "ordering: an inverted frequency band is refused",
        expect: Expect::Killed,
    },
];

fn main() -> std::process::ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let filter = std::env::args().nth(1);

    // A clean tree is the safety net. Everything below restores the file it
    // edited, but a kill -9 between write and restore cannot, and then the next
    // thing to run is a mutated workspace. Starting clean means the damage is
    // whatever `git status` shows and `git checkout --` undoes.
    if !git_is_clean(&root) {
        eprintln!(
            "mutate: the working tree is dirty.\n\
             This edits files in place and puts them back; starting clean is what makes an \
             interrupted run recoverable with `git checkout --`. Commit or stash first."
        );
        return std::process::ExitCode::FAILURE;
    }

    let chosen: Vec<&Mutant> = MUTANTS
        .iter()
        .filter(|m| {
            filter
                .as_ref()
                .is_none_or(|f| m.claim.contains(f.as_str()) || m.file.contains(f.as_str()))
        })
        .collect();
    if chosen.is_empty() {
        eprintln!("mutate: no mutant matches {filter:?}");
        return std::process::ExitCode::FAILURE;
    }

    println!(
        "{} mutants, each judged by its own crate's tests\n",
        chosen.len()
    );
    let mut report = String::new();
    let mut unexpected = 0usize;

    for m in chosen {
        let path = root.join(m.file);
        let original = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

        // Trap 1, and the way trap 2 announces itself.
        let hits = original.matches(m.from).count();
        if hits != 1 {
            unexpected += 1;
            let _ = writeln!(
                report,
                "  BROKEN   {}\n           the pattern matches {hits} times in {} — a rewrap or a \
                 rewrite moved it, and no test was run",
                m.claim, m.file
            );
            println!("  BROKEN   {}", m.claim);
            continue;
        }

        std::fs::write(&path, original.replacen(m.from, m.to, 1))
            .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        let outcome = run_suite(&root, package_of(m.file));
        std::fs::write(&path, &original)
            .unwrap_or_else(|e| panic!("restore {}: {e}", path.display()));

        let (label, unexpected_here) = match (&outcome, &m.expect) {
            (Outcome::Uncompilable, _) => ("BROKEN ", true),
            (Outcome::Failed, Expect::Killed) => ("killed ", false),
            (Outcome::Passed, Expect::Survives(_)) => ("equiv  ", false),
            (Outcome::Passed, Expect::Killed) => ("SURVIVED", true),
            (Outcome::Failed, Expect::Survives(_)) => ("KILLED ", true),
        };
        if unexpected_here {
            unexpected += 1;
            let _ = writeln!(
                report,
                "  {label} {}\n           {}",
                m.claim,
                explain(&outcome, &m.expect)
            );
        }
        println!("  {label} {}", m.claim);
    }

    println!();
    if unexpected == 0 {
        println!("every mutant did what it was supposed to.");
    } else {
        println!("{unexpected} mutant(s) did not:\n{report}");
    }

    // Belt and braces: if a write failed silently or a panic skipped a restore,
    // the tree says so and the run must not read as clean.
    if !git_is_clean(&root) {
        eprintln!(
            "mutate: the tree is dirty at exit — a mutation was NOT put back. \
             `git status` to see it, `git checkout -- <file>` to undo."
        );
        return std::process::ExitCode::FAILURE;
    }
    if unexpected == 0 {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

enum Outcome {
    /// Something in the suite failed — the mutant was noticed.
    Failed,
    /// Everything passed — nothing noticed.
    Passed,
    /// It never got as far as running. Trap 3: this is a non-zero exit that
    /// looks identical to a kill and means the opposite, since a mutant that
    /// does not build put no changed behaviour in front of any test.
    Uncompilable,
}

/// The crate that owns a source file — its first path component, so there is no
/// second field to fall out of step with the path.
fn package_of(file: &str) -> &str {
    match file.split_once('/') {
        Some((pkg, _)) if pkg != "src" => pkg,
        // `src/...` is the root crate, which is named for the repository.
        _ => "utterance",
    }
}

fn run_suite(root: &Path, package: &str) -> Outcome {
    let out = Command::new("cargo")
        .args(["test", "-p", package, "--quiet"])
        .current_dir(root)
        .output()
        .expect("run cargo test");
    if out.status.success() {
        return Outcome::Passed;
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("error[E") || stderr.contains("could not compile") {
        Outcome::Uncompilable
    } else {
        Outcome::Failed
    }
}

fn explain(outcome: &Outcome, expect: &Expect) -> String {
    match (outcome, expect) {
        (Outcome::Uncompilable, _) => {
            "the mutant does not compile, so nothing was tested — rewrite it as a change that \
             builds"
                .to_string()
        }
        (Outcome::Passed, Expect::Killed) => {
            "the crate's own suite passed with this broken. Re-run it with `cargo test \
             --workspace` before concluding: if something further out catches it, the gap is \
             that the pure crate is relying on an integration test three layers away. If \
             nothing does, either a test is missing or the mutant is equivalent — decide \
             which, and if it is equivalent say so in `expect` with the argument."
                .to_string()
        }
        (Outcome::Failed, Expect::Survives(why)) => {
            format!("declared equivalent, and something failed. The argument was: {why}")
        }
        _ => String::new(),
    }
}

fn git_is_clean(root: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .is_ok_and(|o| o.status.success() && o.stdout.is_empty())
}
