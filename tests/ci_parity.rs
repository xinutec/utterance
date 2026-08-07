//! CI runs the gate's list, or says why it does not.
//!
//! `.github/workflows/build.yml` reproduces a subset of `gate.dhall` by hand,
//! and three one-line breakages in two days came from the same place: the gate
//! gained a row and the workflow did not. `cargo fmt --check` and
//! `typecheck:e2e` were gate-only for a while, so they ran on whichever machine
//! happened to fire the pre-commit hook and on nothing else; and when the table
//! made `ng build` a row of its own, `fe-verify` had nothing to serve and
//! Playwright died on `Timed out waiting 60000ms from config.webServer` for two
//! commits.
//!
//! The gate is a machine-readable table now, which is the ingredient that was
//! missing. This does not *generate* the workflow: three rows are implemented
//! differently here on purpose — the runner has no nix, no dhall and no token
//! for the private dev-lint repo — and a generator would have to carry those
//! exceptions as data anyway, so the same list would still have to be right. It
//! asserts the two agree, which is the part that kept going wrong.
//!
//! **The link between them is the step name.** A workflow step whose `name` is
//! exactly a gate row's name *is* that row; every other step is something CI
//! does for its own reasons — checkout, a toolchain, a browser — and is ignored.
//! Names rather than commands, because three rows legitimately run something
//! else here, so a check comparing argv would fire on all three every run, and a
//! check that is wrong every run gets switched off within a week.
//!
//! This is a `cargo test` rather than a gate row of its own, so it runs in both
//! places at once: in the gate, and in the CI it is checking.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use saphyr::{LoadableYamlNode, Yaml};

/// Gate rows the workflow deliberately does not run, each with its reason.
///
/// A reason is mandatory, and one that no longer names a row is itself a
/// failure: a waiver outliving its subject reads as coverage.
const NOT_IN_CI: &[(&str, &str)] = &[
    (
        "the table matches its Dhall",
        "re-rendering needs dhall and the nix dev shell, and the runner has neither. \
         The JSON is generated and committed, so what CI runs is the same table this \
         row would check.",
    ),
    (
        "dev-lint",
        "needs nix in the runner and a token for the private sibling repo. Same split \
         as messages and coach: the shared fleet rules stay local across the whole \
         fleet, and CI is the rest of the gate.",
    ),
];

fn repo(path: &str) -> String {
    let full = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&full).unwrap_or_else(|e| panic!("read {}: {e}", full.display()))
}

/// The gate's rows, in table order.
fn gate_rows() -> Vec<String> {
    let table: serde_json::Value =
        serde_json::from_str(&repo("gate.json")).expect("parse gate.json");
    let checks = table["checks"]
        .as_array()
        .expect("gate.json has no `checks` array");
    assert!(!checks.is_empty(), "gate.json lists no checks at all");
    checks
        .iter()
        .map(|c| {
            c["name"]
                .as_str()
                .expect("a check with no `name`")
                .to_owned()
        })
        .collect()
}

/// A mapping key, or `None` if this node is not a mapping or has no such key.
///
/// Not `node["key"]`: saphyr's `Index` panics on a key that is not there, and
/// most of what is asked for below is optional by design — a step with no
/// `name`, a job with no `steps`.
fn field<'y, 'src>(node: &'y Yaml<'src>, key: &str) -> Option<&'y Yaml<'src>> {
    node.as_mapping()?
        .iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .map(|(_, v)| v)
}

/// Each job's named steps, both lists in file order.
///
/// Unnamed steps are dropped rather than counted: a step with no `name` makes no
/// claim about which row it is.
fn workflow_jobs() -> Vec<(String, Vec<String>)> {
    let src = repo(".github/workflows/build.yml");
    let docs = Yaml::load_from_str(&src).expect("parse build.yml");
    let doc = docs.first().expect("build.yml is empty");
    let jobs = field(doc, "jobs")
        .and_then(Yaml::as_mapping)
        .expect("build.yml has no `jobs` mapping");
    assert!(!jobs.is_empty(), "build.yml defines no jobs at all");
    jobs.iter()
        .map(|(id, body)| {
            let names = field(body, "steps")
                .and_then(Yaml::as_sequence)
                .map(|steps| {
                    steps
                        .iter()
                        .filter_map(|step| field(step, "name").and_then(Yaml::as_str))
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            (
                id.as_str()
                    .expect("a job id that is not a string")
                    .to_owned(),
                names,
            )
        })
        .collect()
}

/// Every gate row name a workflow step claims, with the job that claims it.
fn claims() -> Vec<(String, String)> {
    let rows = gate_rows();
    workflow_jobs()
        .into_iter()
        .flat_map(|(job, names)| {
            names
                .into_iter()
                .filter(|n| rows.contains(n))
                .map(move |n| (n, job.clone()))
        })
        .collect()
}

#[test]
fn every_gate_row_runs_in_ci_or_says_why_not() {
    let claimed: Vec<String> = claims().into_iter().map(|(row, _)| row).collect();
    let missing: Vec<String> = gate_rows()
        .into_iter()
        .filter(|row| !claimed.contains(row) && !NOT_IN_CI.iter().any(|(w, _)| w == row))
        .collect();

    assert!(
        missing.is_empty(),
        "these gate rows run nowhere in CI: {missing:?}\n\
         Add a step to .github/workflows/build.yml named exactly after the row, or \
         add the row to NOT_IN_CI here with the reason it cannot run on a runner."
    );
}

#[test]
fn no_waiver_outlives_its_row() {
    let rows = gate_rows();
    let stale: Vec<&str> = NOT_IN_CI
        .iter()
        .map(|(row, _)| *row)
        .filter(|row| !rows.iter().any(|r| r == row))
        .collect();

    assert!(
        stale.is_empty(),
        "NOT_IN_CI excuses rows the gate no longer has: {stale:?}\n\
         The row was renamed or dropped; drop the excuse with it."
    );

    for (row, why) in NOT_IN_CI {
        assert!(!why.trim().is_empty(), "`{row}` is waived with no reason");
    }

    // The other direction, which the check above cannot see: a row excused as
    // unrunnable that CI turns out to run. Whoever got it working on a runner
    // left the reason behind saying it cannot be done, and the next person reads
    // that instead of the workflow.
    let claimed: Vec<String> = claims().into_iter().map(|(row, _)| row).collect();
    let contradicted: Vec<&str> = NOT_IN_CI
        .iter()
        .map(|(row, _)| *row)
        .filter(|row| claimed.iter().any(|c| c == row))
        .collect();

    assert!(
        contradicted.is_empty(),
        "NOT_IN_CI says these cannot run on a runner, and CI runs them: {contradicted:?}\n\
         Drop the excuse — it is now the opposite of true."
    );
}

#[test]
fn a_row_is_claimed_once() {
    let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (row, job) in claims() {
        seen.entry(row).or_default().push(job);
    }
    let doubled: Vec<(&String, &Vec<String>)> =
        seen.iter().filter(|(_, jobs)| jobs.len() > 1).collect();

    assert!(
        doubled.is_empty(),
        "one gate row, claimed by more than one CI step: {doubled:?}\n\
         Two steps under one name is a copy-paste, and it makes the run twice as \
         long for no extra coverage."
    );
}

#[test]
fn ci_runs_the_covered_rows_in_gate_order() {
    // The gate's own schema calls row order presentation rather than dependency,
    // and for failure reporting it is — every row runs whatever came before.
    // That stops being true the moment one row writes an artifact another reads.
    // `frontend build` writes `dist/` and `frontend ui-check` serves it; inverting
    // those two is exactly the breakage that cost two commits, and in a workflow
    // nothing else would have noticed.
    let rows = gate_rows();
    for (job, names) in workflow_jobs() {
        let order: Vec<usize> = names
            .iter()
            .filter_map(|name| rows.iter().position(|row| row == name))
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();

        assert_eq!(
            order,
            sorted,
            "job `{job}` runs gate rows out of table order: {:?}",
            order.iter().map(|&i| &rows[i]).collect::<Vec<_>>()
        );
    }
}
