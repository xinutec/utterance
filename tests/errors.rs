//! The error codes, held to the one thing about them the compiler cannot check.
//!
//! Everything else is now a type: a route cannot invent a code, `webauth` and
//! the API share one list, and the browser reads that list as a union so a
//! comparison against a code that does not exist stops its build. What is left
//! is `ErrorCode::name`, which restates the serde attribute because the log line
//! and the tests want a `&str`.

use utterance::error::ErrorCode;

/// `name` and the serde attribute are two spellings of the same thing.
///
/// Serialisation is what a client actually receives, so it is the authority
/// here and `name` is what is checked against it. Drift between them would show
/// up as a log line naming a code no client was ever sent.
#[test]
fn name_matches_what_goes_on_the_wire() {
    for code in ErrorCode::ALL {
        let json = serde_json::to_value(code).expect("a unit variant always serialises");
        assert_eq!(
            json.as_str(),
            Some(code.name()),
            "{code:?} serialises as {json} and names itself {:?}",
            code.name()
        );
    }
}

/// No two codes share a spelling.
///
/// They cannot, being variants of one enum — unless a `rename` is added, which
/// is the only way to reintroduce the collision. A client branching on a code
/// two failures share would take the wrong branch for one of them, and which
/// one would depend on the request.
#[test]
fn every_code_is_distinct() {
    let mut names: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.name()).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count, "two codes share a spelling");
}

/// [`ErrorCode::ALL`] really is all of them.
///
/// The array's length is written down, so a variant added without extending it
/// fails to compile — but one *swapped* for another would not. This walks the
/// list and checks each entry appears once, which is the property `ALL` is used
/// for above.
#[test]
fn all_lists_each_code_once() {
    for code in ErrorCode::ALL {
        let found = ErrorCode::ALL.iter().filter(|c| **c == code).count();
        assert_eq!(found, 1, "{code:?} appears {found} times in ALL");
    }
}
