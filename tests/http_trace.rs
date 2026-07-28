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

// ---- the layer's position in the stack ------------------------------------

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;
use utterance::config::Config;
use utterance::state::AppState;
use utterance::store::Store;
use utterance::webauth::WebAuth;

/// A writer that keeps everything, so a test can read back what was logged.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log buffer").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn a_refused_request_is_still_logged() {
    // The defect this was deployed with, found by reading the log rather than
    // by reasoning about it: tracing was applied *inside* the sign-in gate, so
    // a 401 short-circuited before anything recorded it — and the requests
    // worth seeing most, the refused ones, were the only ones missing.
    let dir = std::env::temp_dir().join(format!("utterance-trace-test-{}", std::process::id()));
    let router = utterance::routes::router_with(
        AppState::new(
            Config {
                bind_addr: "127.0.0.1:0".into(),
                data_dir: dir.clone(),
                static_dir: None,
            },
            Store::open(&dir).expect("open store"),
        ),
        Some(Arc::new(WebAuth::new("secret", "id", "shh", []))),
    );

    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(captured.clone())
        .with_ansi(false)
        .finish();

    let response = {
        let _guard = tracing::subscriber::set_default(subscriber);
        router
            .oneshot(
                Request::get("/api/controls?mapping=tonnetz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("router answered")
    };
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let logged = String::from_utf8(captured.0.lock().expect("log buffer").clone()).expect("utf8");
    assert!(
        logged.contains("/api/controls?mapping=tonnetz"),
        "a refused request left no line:\n{logged}"
    );
    assert!(
        logged.contains("401"),
        "the refusal was logged without its status:\n{logged}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
