//! **`index.html` must revalidate; the hashed bundle may be kept forever.**
//!
//! WHY THIS IS A TEST AND NOT A CURL. Six apps were given this header on
//! 2026-08-14 and the task was closed as "all fixed, deployed and curled". No
//! standing check went with it, so when three more names were measured on
//! 2026-09-07 — utterance among them, live since 2026-07-28 — nothing had ever
//! asked them the question. A header verified by hand once is a header nobody
//! is watching.
//!
//! What goes wrong without it: with no `Cache-Control` a client falls back to
//! HEURISTIC freshness, roughly a tenth of the document's age, and may keep
//! `index.html` for days without asking. That document names the content-hashed
//! bundle, so the new `main-*.js` is never fetched either and the deploy is
//! invisible. An Android `WebView` ran several builds behind for hours with a
//! missing button as the only symptom.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;
use utterance::config::Config;
use utterance::routes;
use utterance::state::AppState;
use utterance::store::Store;

/// Data dir and static dir together, so one `Drop` clears both. The static dir
/// is shaped like a real `ng build` output: the document, and one asset whose
/// NAME carries the content hash.
struct TestApp {
    router: axum::Router,
    dir: std::path::PathBuf,
}

impl TestApp {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "utterance-cache-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let static_dir = dir.join("static");
        std::fs::create_dir_all(&static_dir).expect("create static dir");
        std::fs::write(
            static_dir.join("index.html"),
            "<!doctype html><html></html>",
        )
        .expect("index");
        std::fs::write(static_dir.join("main-HFJIWTLG.js"), "export {};").expect("bundle");

        let cfg = Config {
            bind_addr: "127.0.0.1:0".into(),
            data_dir: dir.clone(),
            static_dir: Some(static_dir),
        };
        Self {
            router: routes::router(AppState::new(cfg, Store::open(&dir).expect("open store"))),
            dir,
        }
    }

    async fn cache_control(self, path: &str) -> (StatusCode, String) {
        let res = self
            .router
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let value = res
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|v| v.to_str().unwrap().to_owned())
            .unwrap_or_default();
        (status, value)
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn the_document_is_asked_for_every_time() {
    let (status, cc) = TestApp::new().cache_control("/index.html").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cc, "no-cache");
}

/// The path that actually matters on a phone. A deep link matches no route, so
/// it reaches the SPA fallback and is served the document — which means the
/// fallback needs the header just as much as the file does, and it is reached
/// by a different arm of `ServeDir`.
#[tokio::test]
async fn a_deep_link_served_the_shell_revalidates_too() {
    let (status, cc) = TestApp::new().cache_control("/recordings").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cc, "no-cache");
}

/// The only kind of response `immutable` is honestly available for: a new build
/// is a new URL, so the old one can never be wrong.
#[tokio::test]
async fn the_content_hashed_bundle_may_be_kept() {
    let (status, cc) = TestApp::new().cache_control("/main-HFJIWTLG.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cc, "public, max-age=31536000, immutable");
}

/// ⚠ **A 304 is not an error, and the difference is not cosmetic.** The guard
/// in `cache_control_for` was first written as `!status.is_success()`, which
/// also caught `304 Not Modified` — and a 304 must carry the headers a 200
/// would, so the client can refresh what it already holds. Without them every
/// revalidated asset became a full re-fetch. In `messages` the symptom was a
/// thread that landed 271px above the bottom, because images arrived and grew
/// the page after it had scrolled: a scroll bug with no visible connection to a
/// cache header. This test is the cheap version of that accident.
#[tokio::test]
async fn a_revalidated_asset_is_still_told_it_may_be_kept() {
    let harness = TestApp::new();
    let app = harness.router.clone();

    let first = app
        .clone()
        .oneshot(
            Request::get("/main-HFJIWTLG.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let etag = first
        .headers()
        .get(header::ETAG)
        .expect("ServeDir sends an ETag, which is what makes a 304 reachable")
        .clone();

    let second = app
        .oneshot(
            Request::get("/main-HFJIWTLG.js")
                .header(header::IF_NONE_MATCH, &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        second
            .headers()
            .get(header::CACHE_CONTROL)
            .map(|v| v.to_str().unwrap()),
        Some("public, max-age=31536000, immutable"),
    );
}

/// **A missing FILE must 404, not be handed the page.**
///
/// #1478, measured 2026-09-08: `GET /media/nope.woff2` came back `200 text/html`
/// — the SPA shell, to a browser that asked for a font. It renders broken icons
/// and reports nothing at all, so the failure is silent on both sides; the wrong
/// answer being a 200 is exactly what makes this invisible.
///
/// The rule is a dot in the last path segment: `/recordings` is a route and
/// `/main-ABC123.js` is a file. A heuristic, and the alternative — enumerating
/// the bundle's own asset names — would have to be rebuilt whenever `ng build`
/// changes a hash. `tasks` and memview's console both landed this same fix.
#[tokio::test]
async fn a_missing_asset_is_a_404_and_not_the_page() {
    let (status, _) = TestApp::new().cache_control("/media/nope.woff2").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The other half, and it is the one a careless fix breaks: a client-side route
/// has no dot and must still load the shell, or every deep link 404s.
#[tokio::test]
async fn a_deep_link_still_gets_the_page() {
    let (status, _) = TestApp::new().cache_control("/recordings").await;
    assert_eq!(status, StatusCode::OK);
}
