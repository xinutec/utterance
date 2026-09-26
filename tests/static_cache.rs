//! **`index.html` must revalidate; the hashed bundle may be kept forever.** A test,
//! because a header checked by hand once is not being watched. Without
//! `Cache-Control` a client can keep `index.html` for days, and with it the old
//! bundle, making a deploy invisible.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;
use utterance::config::Config;
use utterance::routes;
use utterance::state::AppState;
use utterance::store::Store;

/// Data dir and static dir together, one `Drop` for both; the static dir looks
/// like `ng build` output.
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

/// A deep link reaches the SPA fallback — a different arm of `ServeDir` — and
/// needs the header just as much.
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

/// ⚠ A 304 must carry the headers a 200 would, or every revalidation becomes a
/// full fetch — which a `!status.is_success()` guard would cause.
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

/// **A missing file must 404, not be handed the page** — a font answered with
/// HTML fails silently on both sides. A file is a name with a dot in its last
/// segment.
#[tokio::test]
async fn a_missing_asset_is_a_404_and_not_the_page() {
    let (status, _) = TestApp::new().cache_control("/media/nope.woff2").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The other half: a client-side route has no dot and must still load the shell.
#[tokio::test]
async fn a_deep_link_still_gets_the_page() {
    let (status, _) = TestApp::new().cache_control("/recordings").await;
    assert_eq!(status, StatusCode::OK);
}
