//! The sign-in gate, over the real router.
//!
//! Every test here builds the gate explicitly rather than through the
//! environment. That is deliberate: `WebAuth::from_env` reads process-wide
//! state, and a test that sets it would raise the wall for every other test
//! running in the same binary — so the suite would be measuring leakage rather
//! than the gate.
//!
//! What is being checked is not "does OAuth work" — that needs a Nextcloud —
//! but the two things that decide whether this is a wall or a decoration: that
//! nothing under `/api` answers without a valid session, and that the wall is
//! completely absent when nobody configured one.

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

/// Every route that reads or changes a recording. Written out rather than
/// derived, because the failure this guards against is a route added later and
/// left outside the gate — which a list generated from the router would inherit
/// instead of catching.
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
    // The routes that matter most: without the gate, anyone could add a
    // recording of anything, or delete the ones that are there.
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

    // Reaching the handler is the assertion. `/api/controls` needs no data, so
    // a 200 here means the gate opened rather than that the store happened to
    // have something in it.
    let (status, body) = get(&app, "/api/controls", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("tonnetz"), "{body}");
}

#[tokio::test]
async fn a_nextcloud_user_who_is_not_on_the_list_is_refused() {
    // The allowlist is the difference between "anyone with an account on the
    // fleet's Nextcloud" and "the two people this is for".
    let auth = gate();
    let cookie = signed_in_as(&auth, "someone-else");
    let app = TestApp::new(Some(auth));

    let (status, body) = get(&app, "/api/controls", Some(&cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body.contains("someone-else"), "{body}");
}

#[tokio::test]
async fn a_cookie_signed_by_someone_else_does_not_open_the_gate() {
    // Someone who knows the cookie's shape but not the secret. If this passes
    // the wall is decoration, since the payload names the user.
    let forger = WebAuth::new("a-different-secret", "client", "shh", []);
    let cookie = signed_in_as(&forger, "pippijn");
    let app = TestApp::new(Some(gate()));

    let (status, _) = get(&app, "/api/controls", Some(&cookie)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_health_check_stays_open() {
    // The cluster probes this before anyone has signed in; gating it would make
    // the pod permanently unready and the app permanently unreachable.
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
    // The state is what makes the callback refuse a request nobody started.
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
    // The property the Mac and every other test depend on. If this fails, the
    // gate has stopped being opt-in and local development needs a Nextcloud.
    let app = TestApp::new(None);
    let (status, body) = get(&app, "/api/controls", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn with_no_sign_in_configured_there_is_nothing_to_sign_in_to() {
    // A /login that redirected to a Nextcloud this deployment never heard of
    // would be worse than absent — it would look like a way in.
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
// Everything above goes through HTTP, which can only ever say "no". These go
// straight at the pair that issues and reads a cookie, because the ways a
// signed credential fails are different from each other and a 401 does not say
// which one happened.

/// A moment far enough in the past that TTLs can be stepped over.
fn a_moment() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
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
    // Seven days. Checked by stepping over it rather than by reading the
    // constant back, so a change to the constant that forgets the expiry check
    // still fails here.
    let auth = gate();
    let now = a_moment();
    let token = auth.issue_session(&session("pippijn"), now);
    let a_week = Duration::from_secs(7 * 24 * 60 * 60);

    assert!(
        auth.read_session(&token, now + a_week - Duration::from_secs(60))
            .is_some()
    );
    assert_eq!(
        auth.read_session(&token, now + a_week + Duration::from_secs(60)),
        None
    );
}

#[tokio::test]
async fn a_payload_swapped_under_a_good_signature_is_refused() {
    // The attack the signature exists for. The payload names the user, so if a
    // cookie could be edited and keep its MAC, anyone signed in as anyone could
    // rewrite themselves into anyone else.
    let auth = gate();
    let now = a_moment();
    let mine = auth.issue_session(&session("michiel"), now);
    let theirs = auth.issue_session(&session("pippijn"), now);
    let (my_payload, my_mac) = mine.split_once('.').expect("a two-part token");
    let (their_payload, their_mac) = theirs.split_once('.').expect("a two-part token");

    // Both halves are real, so this cannot pass by being malformed — which is
    // how a test like this quietly stops testing anything.
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
    // This reads a value someone else chooses, so every malformed shape has to
    // have an answer, and the answer has to be "no" rather than a stack trace.
    let auth = gate();
    let now = a_moment();
    for token in ["", ".", "a.b", "no-dot", "!!.??", "....", "ᚠ.ᚠ"] {
        assert_eq!(auth.read_session(token, now), None, "{token:?}");
    }
}

#[tokio::test]
async fn only_a_local_path_survives_as_a_return_target() {
    // Anything that could leave this origin turns signing in into an open
    // redirect, which is a phishing primitive rather than a small bug.
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
    // The documented meaning of "no list", and the one that would be a security
    // hole if it were read the other way round by accident.
    let open = WebAuth::new("secret", "client", "shh", []);
    assert!(open.permits("anyone"));
    assert!(gate().permits("pippijn"));
    assert!(!gate().permits("anyone"));
}

#[tokio::test]
async fn a_server_call_presents_the_public_host_when_the_address_differs() {
    // The hairpin fix. On the cluster Nextcloud's public name resolves to the
    // node's own IP, which a pod cannot open — so the call goes to the in-cluster
    // Service and carries the public host, or Nextcloud refuses it as an
    // untrusted domain.
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

    // ...and no Host header when there is nothing to pretend about, since
    // sending one needlessly is a way to break a working deployment.
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
    // The redirect URI contains slashes and colons and the state is base64 with
    // a dot in it. Unescaped, either would end the parameter early and the
    // sign-in would fail in a way that looks like Nextcloud's fault.
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
