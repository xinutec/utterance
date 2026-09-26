//! The sign-in gate, over the real router.
//!
//! Not "does OAuth work" — that needs a Nextcloud — but whether this is a wall:
//! nothing under `/api` answers without a valid session, and the wall is absent
//! when nobody configured one. Every test builds the gate explicitly; setting
//! the environment would raise it for every other test in the binary.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use utterance::config::Config;
use utterance::routes;
use utterance::state::AppState;
use utterance::store::Store;
use utterance::webauth;
use utterance::webauth::{Session, WebAuth};

struct TestApp {
    router: Router,
    dir: std::path::PathBuf,
}

impl TestApp {
    /// A router with sign-in configured, or without it when `auth` is `None`.
    fn new(auth: Option<WebAuth>) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "utterance-webauth-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let cfg = Config {
            bind_addr: "127.0.0.1:0".into(),
            data_dir: dir.clone(),
            static_dir: None,
        };
        Self {
            router: routes::router_with(
                AppState::new(cfg, Store::open(&dir).expect("open store")),
                auth.map(Arc::new),
            ),
            dir,
        }
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The gate as it is deployed: two people named, nobody else.
fn gate() -> WebAuth {
    WebAuth::new(
        "a-test-secret",
        "client",
        "shh",
        ["pippijn".to_string(), "michiel".to_string()],
    )
}

fn signed_in_as(auth: &WebAuth, user: &str) -> String {
    let token = auth.issue_session(
        &Session {
            user_id: user.to_string(),
            display_name: user.to_string(),
        },
        SystemTime::now(),
    );
    format!("{}={token}", WebAuth::COOKIE)
}

async fn send(app: &TestApp, request: Request<Body>) -> (StatusCode, String) {
    let response = app
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("router did not answer");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

async fn get(app: &TestApp, path: &str, cookie: Option<&str>) -> (StatusCode, String) {
    let mut request = Request::get(path);
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    send(app, request.body(Body::empty()).unwrap()).await
}

/// Every route that reads or changes a recording, written out: a list derived
/// from the router would inherit a route added outside the gate.
const GUARDED: [(&str, &str); 6] = [
    ("GET", "/api/recordings"),
    ("GET", "/api/recordings/abc"),
    ("GET", "/api/recordings/abc/audio"),
    ("GET", "/api/recordings/abc/render"),
    ("GET", "/api/voice"),
    ("GET", "/api/controls"),
];

#[tokio::test]
async fn nothing_under_api_answers_without_a_session() {
    let app = TestApp::new(Some(gate()));
    for (method, path) in GUARDED {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let (status, body) = send(&app, request).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}: {body}");
        let json: Value = serde_json::from_str(&body).expect("a JSON error body");
        assert_eq!(json["code"], "not_authenticated", "{method} {path}");
    }
}

#[tokio::test]
async fn uploading_and_deleting_are_gated_too() {
    // Without the gate, anyone could upload or delete.
    let app = TestApp::new(Some(gate()));
    for request in [
        Request::post("/api/recordings?label=x").body(Body::from(vec![0u8; 16])),
        Request::delete("/api/recordings/abc").body(Body::empty()),
    ] {
        let (status, body) = send(&app, request.unwrap()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    }
}

#[tokio::test]
async fn a_signed_in_user_is_let_through() {
    let auth = gate();
    let cookie = signed_in_as(&auth, "pippijn");
    let app = TestApp::new(Some(auth));

    // `/api/controls` needs no data, so a 200 means the gate opened.
    let (status, body) = get(&app, "/api/controls", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("tonnetz"), "{body}");
}

#[tokio::test]
async fn a_nextcloud_user_who_is_not_on_the_list_is_refused() {
    // The allowlist narrows "any fleet Nextcloud account" to the people named.
    let auth = gate();
    let cookie = signed_in_as(&auth, "someone-else");
    let app = TestApp::new(Some(auth));

    let (status, body) = get(&app, "/api/controls", Some(&cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body.contains("someone-else"), "{body}");
}

#[tokio::test]
async fn a_cookie_signed_by_someone_else_does_not_open_the_gate() {
    // The cookie's shape without the secret must not pass.
    let forger = WebAuth::new("a-different-secret", "client", "shh", []);
    let cookie = signed_in_as(&forger, "pippijn");
    let app = TestApp::new(Some(gate()));

    let (status, _) = get(&app, "/api/controls", Some(&cookie)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_health_check_stays_open() {
    // The cluster probes it before anyone signs in.
    let app = TestApp::new(Some(gate()));
    let (status, body) = get(&app, "/healthz", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "ok");
}

#[tokio::test]
async fn signing_in_sends_the_browser_to_nextcloud() {
    let app = TestApp::new(Some(gate()));
    let response = app
        .router
        .clone()
        .oneshot(
            Request::get("/login?return_to=/compare")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response.headers()[header::LOCATION].to_str().unwrap();
    assert!(
        location.starts_with("https://dash.xinutec.org/"),
        "{location}"
    );
    assert!(location.contains("apps/oauth2/authorize"), "{location}");
    assert!(location.contains("response_type=code"), "{location}");
    // The state makes the callback refuse a request nobody started.
    assert!(location.contains("state="), "{location}");
}

#[tokio::test]
async fn a_callback_nobody_started_is_refused() {
    let app = TestApp::new(Some(gate()));
    let (status, body) = get(&app, "/auth/callback?code=abc&state=forged", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body.contains("bad_login_state"), "{body}");
}

#[tokio::test]
async fn with_no_sign_in_configured_the_app_is_wide_open() {
    // Local development and every other test rely on the gate being opt-in.
    let app = TestApp::new(None);
    let (status, body) = get(&app, "/api/controls", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn with_no_sign_in_configured_there_is_nothing_to_sign_in_to() {
    // A /login to a Nextcloud nobody configured would look like a way in.
    let app = TestApp::new(None);
    for path in ["/login", "/auth/callback", "/api/me"] {
        let (status, _) = get(&app, path, None).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{path} exists without a gate"
        );
    }
}

#[tokio::test]
async fn who_am_i_answers_for_a_signed_in_user() {
    let auth = gate();
    let cookie = signed_in_as(&auth, "michiel");
    let app = TestApp::new(Some(auth));

    let (status, body) = get(&app, "/api/me", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let json: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["uid"], "michiel");
}

// ---- the credential itself ------------------------------------------------
//
// Straight at the pair that issues and reads a cookie: a 401 cannot say which
// of the ways a signed credential fails happened.

/// A moment far enough in the past that TTLs can be stepped over.
fn a_moment() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_hours(500_000)
}

fn session(user: &str) -> Session {
    Session {
        user_id: user.to_string(),
        display_name: user.to_string(),
    }
}

#[tokio::test]
async fn a_cookie_reads_back_as_the_person_it_was_issued_to() {
    let auth = gate();
    let now = a_moment();
    let token = auth.issue_session(&session("pippijn"), now);
    assert_eq!(auth.read_session(&token, now), Some(session("pippijn")));
}

#[tokio::test]
async fn a_cookie_signed_with_another_secret_reads_as_nothing() {
    let now = a_moment();
    let token =
        WebAuth::new("another-secret", "client", "shh", []).issue_session(&session("x"), now);
    assert_eq!(gate().read_session(&token, now), None);
}

#[tokio::test]
async fn a_cookie_stops_being_accepted_once_it_expires() {
    // Seven days, checked by stepping over it rather than reading the constant.
    let auth = gate();
    let now = a_moment();
    let token = auth.issue_session(&session("pippijn"), now);
    let a_week = Duration::from_hours(168);

    assert!(
        auth.read_session(&token, now + a_week - Duration::from_mins(1))
            .is_some()
    );
    assert_eq!(
        auth.read_session(&token, now + a_week + Duration::from_mins(1)),
        None
    );
}

#[tokio::test]
async fn a_payload_swapped_under_a_good_signature_is_refused() {
    // An edited payload under a valid MAC would let anyone become anyone.
    let auth = gate();
    let now = a_moment();
    let mine = auth.issue_session(&session("michiel"), now);
    let theirs = auth.issue_session(&session("pippijn"), now);
    let (my_payload, my_mac) = mine.split_once('.').expect("a two-part token");
    let (their_payload, their_mac) = theirs.split_once('.').expect("a two-part token");

    // Both halves are real, so this cannot pass by being malformed.
    assert!(auth.read_session(&mine, now).is_some());
    assert!(auth.read_session(&theirs, now).is_some());

    // Their identity, my signature, and the other way round.
    assert_eq!(
        auth.read_session(&format!("{their_payload}.{my_mac}"), now),
        None,
        "a payload lifted onto another signature was accepted"
    );
    assert_eq!(
        auth.read_session(&format!("{my_payload}.{their_mac}"), now),
        None
    );
}

#[tokio::test]
async fn rubbish_is_refused_rather_than_panicking() {
    // An attacker chooses this value: every malformed shape must answer "no".
    let auth = gate();
    let now = a_moment();
    for token in ["", ".", "a.b", "no-dot", "!!.??", "....", "ᚠ.ᚠ"] {
        assert_eq!(auth.read_session(token, now), None, "{token:?}");
    }
}

#[tokio::test]
async fn only_a_local_path_survives_as_a_return_target() {
    // Anything leaving this origin would make sign-in an open redirect.
    assert_eq!(
        utterance::webauth::safe_return_to(Some("/compare")),
        "/compare"
    );
    for hostile in [
        "//evil.example",
        "https://evil.example",
        "javascript:alert(1)",
        "",
    ] {
        assert_eq!(
            utterance::webauth::safe_return_to(Some(hostile)),
            "/",
            "{hostile}"
        );
    }
    assert_eq!(utterance::webauth::safe_return_to(None), "/");
}

#[tokio::test]
async fn an_empty_allowlist_admits_any_nextcloud_user() {
    // An empty list admits anyone — the documented meaning, and a hole if read
    // the other way.
    let open = WebAuth::new("secret", "client", "shh", []);
    assert!(open.permits("anyone"));
    assert!(gate().permits("pippijn"));
    assert!(!gate().permits("anyone"));
}

#[tokio::test]
async fn a_server_call_presents_the_public_host_when_the_address_differs() {
    // In-cluster, the public name hairpins, so the call goes to the Service and
    // carries the public host, which Nextcloud checks.
    let auth = gate().with_nextcloud(
        "https://dash.example",
        "http://nextcloud-server.nextcloud.svc.cluster.local",
        "https://utterance.example/auth/callback",
    );
    let (url, host) = auth.server_call("/ocs/v2.php/cloud/user");
    assert_eq!(
        url,
        "http://nextcloud-server.nextcloud.svc.cluster.local/ocs/v2.php/cloud/user"
    );
    assert_eq!(host.as_deref(), Some("dash.example"));

    // ...and no Host header when there is only one address.
    let same = gate().with_nextcloud(
        "https://dash.example",
        "https://dash.example",
        "https://x/cb",
    );
    assert_eq!(
        same.server_call("/x"),
        ("https://dash.example/x".into(), None)
    );
}

#[tokio::test]
async fn the_authorize_url_escapes_what_it_interpolates() {
    // The redirect URI and the base64 state must be escaped, or the parameter
    // ends early.
    let auth = gate().with_nextcloud(
        "https://dash.example",
        "https://dash.example",
        "https://utterance.example/auth/callback",
    );
    let url = auth.authorize_url("a b&c");
    assert!(url.contains("state=a%20b%26c"), "{url}");
    assert!(
        url.contains("redirect_uri=https%3A%2F%2Futterance.example%2Fauth%2Fcallback"),
        "{url}"
    );
}

// ---- reading the configuration ------------------------------------------
//
// `from_vars` takes the lookup, so these decisions are testable without the
// process environment.

/// A lookup over a fixed table, standing in for the environment.
fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let owned: Vec<(String, String)> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |name| {
        owned
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }
}

/// The three that decide whether there is a gate at all.
fn configured() -> Vec<(&'static str, &'static str)> {
    vec![
        (webauth::SESSION_SECRET_ENV, "s3cret"),
        (webauth::CLIENT_ID_ENV, "client"),
        (webauth::CLIENT_SECRET_ENV, "shh"),
    ]
}

#[test]
fn nothing_set_means_no_gate() {
    assert!(WebAuth::from_vars(vars(&[])).is_none());
}

#[test]
fn a_half_set_configuration_is_off_rather_than_open() {
    // A wall that can be bypassed is worse than none: each of the three missing
    // in turn.
    for missing in 0..3 {
        let mut set = configured();
        let dropped = set.remove(missing);
        assert!(
            WebAuth::from_vars(vars(&set)).is_none(),
            "a gate was built with {} missing",
            dropped.0
        );
    }
}

#[test]
fn an_empty_string_is_not_a_setting() {
    // A secret exported as "" would sign sessions with the empty key.
    let mut set = configured();
    set[0].1 = "";
    assert!(WebAuth::from_vars(vars(&set)).is_none());
}

#[test]
fn all_three_set_means_a_gate() {
    assert!(WebAuth::from_vars(vars(&configured())).is_some());
}

#[test]
fn a_trailing_slash_does_not_double_up_in_a_url() {
    // A trailing slash would give Nextcloud a `//` path it redirects.
    let mut set = configured();
    set.push((webauth::NC_BASE_URL_ENV, "https://dash.example/"));
    let auth = WebAuth::from_vars(vars(&set)).expect("configured");
    assert_eq!(
        auth.server_call("/ocs"),
        ("https://dash.example/ocs".into(), None)
    );
}

#[test]
fn without_an_internal_url_calls_go_to_the_public_one() {
    let mut set = configured();
    set.push((webauth::NC_BASE_URL_ENV, "https://dash.example"));
    let auth = WebAuth::from_vars(vars(&set)).expect("configured");
    // No `Host` override, because there is only one address in play.
    assert_eq!(
        auth.server_call("/ocs"),
        ("https://dash.example/ocs".into(), None)
    );
}

#[test]
fn an_internal_url_is_where_the_call_goes_and_the_public_one_is_the_host() {
    // In-cluster, the public name hairpins to the node itself.
    let mut set = configured();
    set.push((webauth::NC_BASE_URL_ENV, "https://dash.example"));
    set.push((webauth::NC_INTERNAL_URL_ENV, "http://nextcloud.nc.svc/"));
    let auth = WebAuth::from_vars(vars(&set)).expect("configured");
    assert_eq!(
        auth.server_call("/ocs"),
        (
            "http://nextcloud.nc.svc/ocs".into(),
            Some("dash.example".into())
        )
    );
}

#[test]
fn an_empty_internal_url_falls_back_rather_than_producing_a_hostless_call() {
    // Declared but blank in a manifest: not a URL of `/ocs`.
    let mut set = configured();
    set.push((webauth::NC_BASE_URL_ENV, "https://dash.example"));
    set.push((webauth::NC_INTERNAL_URL_ENV, ""));
    let auth = WebAuth::from_vars(vars(&set)).expect("configured");
    assert_eq!(
        auth.server_call("/ocs"),
        ("https://dash.example/ocs".into(), None)
    );
}

#[test]
fn an_unset_allowlist_admits_any_nextcloud_user() {
    let auth = WebAuth::from_vars(vars(&configured())).expect("configured");
    assert!(auth.permits("anyone"));
}

#[test]
fn the_allowlist_is_split_trimmed_and_stripped_of_blanks() {
    // Hand-written in a manifest: spaces and a trailing comma must not refuse
    // the person named.
    let mut set = configured();
    set.push((webauth::ALLOWED_USERS_ENV, " pippijn, michiel ,, "));
    let auth = WebAuth::from_vars(vars(&set)).expect("configured");
    assert!(auth.permits("pippijn"));
    assert!(auth.permits("michiel"));
    assert!(!auth.permits(""));
    assert!(!auth.permits("someone-else"));
}
