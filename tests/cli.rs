//! The command line, such as it is.
//!
//! Small, and worth a test for one reason: the failure it replaces was silent.
//! The program ignored every argument, so `utterance --help` started a server, sat
//! there looking hung, and then failed to bind because one was already running.
//! Nothing about that says "this program has no options".

use utterance::config::{Invocation, invocation};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn no_arguments_means_run_the_server() {
    assert_eq!(invocation(args(&[])), Ok(Invocation::Serve));
}

#[test]
fn help_prints_instead_of_starting_anything() {
    let Ok(Invocation::Print(text)) = invocation(args(&["--help"])) else {
        panic!("--help did not ask to print");
    };
    // The whole point of the text: every setting is an environment variable, so
    // a help message that did not name them would leave someone believing the
    // program cannot be configured at all.
    for var in ["BIND_ADDR", "DATA_DIR", "STATIC_DIR"] {
        assert!(text.contains(var), "help does not mention {var}:\n{text}");
    }
    assert_eq!(invocation(args(&["-h"])), invocation(args(&["--help"])));
}

#[test]
fn the_help_quotes_the_defaults_the_code_actually_uses() {
    // A usage message listing a default the program does not use is worse than
    // none, because it is believed. These come from the same constants
    // `Config::from_env` reads.
    let Ok(Invocation::Print(text)) = invocation(args(&["--help"])) else {
        panic!("no help");
    };
    unsafe {
        std::env::remove_var("BIND_ADDR");
        std::env::remove_var("DATA_DIR");
    }
    let defaults = utterance::config::Config::from_env();
    assert!(
        text.contains(&defaults.bind_addr),
        "help does not quote the real default address {}",
        defaults.bind_addr
    );
    assert!(
        text.contains(&defaults.data_dir.display().to_string()),
        "help does not quote the real default data directory"
    );
}

#[test]
fn version_answers_with_the_version() {
    let Ok(Invocation::Print(text)) = invocation(args(&["--version"])) else {
        panic!("--version did not ask to print");
    };
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
}

#[test]
fn an_argument_it_does_not_know_is_refused_rather_than_ignored() {
    // The behaviour being fixed. Ignoring an unknown flag means a typo silently
    // does something else, confidently.
    let refused = invocation(args(&["--sereve"])).expect_err("a typo was accepted");
    assert!(refused.contains("--sereve"), "{refused}");
    // ...and says what it does accept, since being told "no" without being told
    // "this instead" is where someone gives up.
    assert!(refused.contains("--help"), "{refused}");
}

#[test]
fn every_argument_is_read_rather_than_only_the_first() {
    // The defect clippy caught: a loop that matches and returns inspects one
    // argument and drops the rest, so a typo after a good flag disappears —
    // which is the very behaviour this module exists to remove.
    let refused =
        invocation(args(&["--version", "--sereve"])).expect_err("a trailing typo was accepted");
    assert!(refused.contains("--sereve"), "{refused}");

    let also_refused =
        invocation(args(&["--help", "--sereve"])).expect_err("a trailing typo was accepted");
    assert!(also_refused.contains("--sereve"), "{also_refused}");
}

#[test]
fn help_wins_over_version_when_both_are_asked_for() {
    let Ok(Invocation::Print(text)) = invocation(args(&["--version", "--help"])) else {
        panic!("no answer");
    };
    assert!(text.contains("Usage:"), "{text}");
}
