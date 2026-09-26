//! CI runs the gate's list, or says why it does not.
//!
//! `.github/workflows/build.yml` restates part of `gate.dhall` by hand, and it
//! breaks when the gate gains a row the workflow lacks. The link is the step
//! name: a step named exactly after a row *is* that row; other steps are CI's
//! own. Names, not commands, since three rows legitimately run differently on a
//! runner (no nix, no dhall, no dev-lint token).
//!
//! A `cargo test`, so it runs both in the gate and in the CI it checks.
//! `DL-GHA-GATE-PARITY` checks coverage fleet-wide, but not on a runner; this
//! also checks that no row is claimed twice and that rows run in table order.
//! The waived rows are read from the workflow's own
//! `# dev-lint: allow-gate-row-not-in-ci-<row>` markers, not listed here.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use saphyr::{LoadableYamlNode, Yaml};

/// The waiver marker whose tail names a gate row CI cannot run.
const WAIVER: &str = "dev-lint: allow-gate-row-not-in-ci-";

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

/// A mapping key, or `None` if absent — saphyr's `Index` panics on a missing key.
fn field<'y, 'src>(node: &'y Yaml<'src>, key: &str) -> Option<&'y Yaml<'src>> {
    node.as_mapping()?
        .iter()
        .find(|(k, _)| k.as_str() == Some(key))
        .map(|(_, v)| v)
}

/// Each job's named steps, in file order; unnamed steps claim no row.
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

/// A row name as its waiver-token tail, the mapping `DL-GHA-GATE-PARITY` uses.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// The row slugs the workflow waives, read off its own waiver markers.
fn waived() -> Vec<String> {
    repo(".github/workflows/build.yml")
        .lines()
        .filter_map(|l| l.split_once(WAIVER))
        .map(|(_, tail)| {
            tail.split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

#[test]
fn every_gate_row_runs_in_ci_or_says_why_not() {
    let claimed: Vec<String> = claims().into_iter().map(|(row, _)| row).collect();
    let waived = waived();
    let missing: Vec<String> = gate_rows()
        .into_iter()
        .filter(|row| !claimed.contains(row) && !waived.contains(&slug(row)))
        .collect();

    assert!(
        missing.is_empty(),
        "these gate rows run nowhere in CI: {missing:?}\n\
         Add a step to .github/workflows/build.yml named exactly after the row, or \
         waive it there with `# {WAIVER}<row>` and the reason a runner cannot."
    );
}

#[test]
fn no_waiver_outlives_its_row() {
    // A waiver for a row the gate no longer has would go unnoticed otherwise:
    // dev-lint does not audit these markers, and does not run on a runner.
    let rows = gate_rows();
    let claimed: Vec<String> = claims().into_iter().map(|(row, _)| row).collect();

    for slug_written in waived() {
        let row = rows.iter().find(|r| slug(r) == slug_written);
        assert!(
            row.is_some(),
            "`{WAIVER}{slug_written}` excuses a row the gate no longer has.\n\
             It was renamed or dropped; drop the excuse with it."
        );
        // A row excused as unrunnable that CI runs: the excuse misleads.
        let row = row.expect("checked just above");
        assert!(
            !claimed.contains(row),
            "`{WAIVER}{slug_written}` says `{row}` cannot run on a runner, and CI \
             runs it.\nDrop the excuse — it is now the opposite of true."
        );
    }
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
    // Order matters once one row reads another's artifact: `frontend build`
    // writes the `dist/` that `frontend ui-check` serves.
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
