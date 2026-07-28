//! What a request line is allowed to say.
//!
//! The interesting half of request logging here is not that it logs, but that
//! two paths must log *less*: a Nextcloud authorization code and a caller-chosen
//! return target both arrive in query strings, and a log outlives the exchange
//! that made them meaningful.

use axum::http::Uri;
use utterance::http_trace::loggable;

#[test]
fn an_ordinary_request_keeps_its_parameters() {
    // These are the point of logging at all: two renders of one take differ
    // only in their query, so a line without it cannot tell them apart.
    let uri: Uri = "/api/recordings/abc/render?mapping=tonnetz&density=0.2"
        .parse()
        .unwrap();
    assert_eq!(
        loggable(&uri),
        "/api/recordings/abc/render?mapping=tonnetz&density=0.2"
    );
}

#[test]
fn a_path_without_a_query_logs_as_itself() {
    assert_eq!(
        loggable(&"/api/controls".parse::<Uri>().unwrap()),
        "/api/controls"
    );
}

#[test]
fn an_authorization_code_never_reaches_the_log() {
    // The failure this exists to prevent. A code is single-use and short-lived,
    // but a log is neither, and this one is read over someone's shoulder on a
    // terminal when something has gone wrong.
    let uri: Uri = "/auth/callback?code=SUPERSECRET&state=abc".parse().unwrap();
    let line = loggable(&uri);
    assert!(!line.contains("SUPERSECRET"), "{line}");
    assert!(!line.contains("state=abc"), "{line}");
    assert_eq!(line, "/auth/callback?<redacted>");
}

#[test]
fn the_login_target_is_redacted_too() {
    // Not itself a credential — but it is attacker-chosen text echoed into a
    // log, and the signed state built from it is not worth writing down either.
    let uri: Uri = "/login?return_to=/compare".parse().unwrap();
    assert_eq!(loggable(&uri), "/login?<redacted>");
}

#[test]
fn a_redacted_line_still_says_there_was_a_query() {
    // A bare path would read as a request that carried no parameters, which is
    // the opposite of what happened — and would hide a malformed callback.
    for path in ["/auth/callback", "/login"] {
        let with: Uri = format!("{path}?x=1").parse().unwrap();
        let without: Uri = path.parse().unwrap();
        assert_ne!(
            loggable(&with),
            loggable(&without),
            "{path}: a query and no query log identically"
        );
    }
}
