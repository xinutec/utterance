//! HTTP routing table.

pub mod api;
pub mod telemetry;

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Response, header};
use axum::routing::{get, post, put};
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::http_trace;
use crate::state::AppState;
use crate::webauth::{self, WebAuth};

/// Largest accepted upload.
///
/// Half a minute of 48 kHz 16-bit stereo is under 6 MB; the headroom covers a
/// long take at 96 kHz without inviting anyone to post a film. axum's 2 MB
/// default would reject a normal recording.
const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

/// How long a static response may be reused without asking again.
///
/// ⚠ **`index.html` MUST REVALIDATE.** With no `Cache-Control` at all — which
/// is what this served from 2026-07-28 until it was measured on 2026-09-07 — a
/// client falls back to HEURISTIC freshness, roughly a tenth of the document's
/// age, and may keep it for days without ever asking. The document names the
/// content-hashed bundle, so the new `main-*.js` is never fetched either and a
/// deploy is invisible: an Android `WebView` loaded messages' API and a whole
/// thread while running several builds behind, with a missing button as the
/// only symptom.
///
/// `no-cache` means "ask first", not "never keep" — the `ETag` still turns the
/// usual case into a 304 with no body.
///
/// Everything else Angular emits carries a content hash in its NAME, so a new
/// build is a new URL and the old one can never be wrong. Those are the one
/// kind of response `immutable` is honestly available for.
///
/// Generic over the body: `ServeDir`'s response body type depends on what it
/// falls back to, and this predicate only ever reads a header.
fn cache_control_for<B>(res: &Response<B>) -> Option<HeaderValue> {
    // ⚠ **A 404 is not an asset.** `SetResponseHeaderLayer::overriding` stamps
    // whatever the service returned, and a missing file answered with a year of
    // `immutable` is a client that will not ask for that name again this year.
    // Only a response that carried something may say how long it keeps.
    //
    // ⚠ NOT `!is_success()`. That excludes **304 Not Modified**, which must
    // carry the headers a 200 would so the client can refresh what it already
    // holds. Stripping it made every revalidated image a full re-fetch, and the
    // arriving bytes grew the thread AFTER it had scrolled to the bottom —
    // `thread-scroll.spec.ts` caught it at 271px off, a symptom with no visible
    // connection to a cache header.
    if res.status().is_client_error() || res.status().is_server_error() {
        return None;
    }
    let is_html = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"));
    Some(if is_html {
        HeaderValue::from_static("no-cache")
    } else {
        HeaderValue::from_static("public, max-age=31536000, immutable")
    })
}

pub fn router(state: AppState) -> Router {
    router_with(state, WebAuth::from_env().map(Arc::new))
}

/// The router with sign-in decided explicitly rather than read from the
/// environment, so a test can raise the gate without setting process-wide state
/// that every other test in the binary would then be running inside.
pub fn router_with(state: AppState, auth: Option<Arc<WebAuth>>) -> Router {
    let api = Router::new()
        .route("/recordings", post(api::upload).get(api::list))
        .route("/recordings/{id}", get(api::detail).delete(api::delete))
        .route("/recordings/{id}/audio", get(api::audio))
        .route("/recordings/{id}/role", put(api::put_role))
        .route("/recordings/{id}/render", get(api::render))
        .route("/recordings/{id}/score", get(api::score))
        .route("/voice", get(api::voice_summary))
        .route("/speaker/corners", get(api::speaker_corners))
        .route("/controls", get(api::controls))
        .route("/telemetry", post(telemetry::record))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES));

    // Applied to the API router alone, so the health check the cluster probes
    // and the sign-in routes themselves stay reachable without a session.
    let api = match &auth {
        Some(gate) => {
            let gate = gate.clone();
            api.layer(axum::middleware::from_fn(move |request, next| {
                webauth::gate(gate.clone(), request, next)
            }))
        }
        None => api,
    };

    // **Outside the gate, and that ordering is the whole point.** A later
    // `layer` wraps the earlier ones, so tracing added before the gate sees
    // only requests the gate let through — and a refused request is exactly the
    // one worth a line. Found by reading the log after deploying it the other
    // way round: `/login` appeared and every 401 was invisible.
    let api = api.layer(http_trace::layer());

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api", api);

    // Only when the gate is up: with no sign-in configured there is nothing for
    // `/login` to do, and a route that redirects to a Nextcloud this deployment
    // never heard of is worse than a 404.
    if let Some(gate) = auth {
        // Traced too, and the reason this module exists: a sign-in that fails
        // silently is the failure nobody can diagnose from the outside.
        app = app.merge(webauth::routes(gate).layer(http_trace::layer()));
    }

    // Serve the built Angular bundle from the same origin, falling back to
    // index.html so client-side routes resolve on reload. API-only when unset,
    // which is the dev arrangement: ng serve holds the app and proxies here.
    if let Some(dir) = state.cfg.static_dir.clone() {
        let index = dir.join("index.html");
        let serve = ServeDir::new(&dir).fallback(ServeFile::new(index));
        // ⚠ The layer wraps the STATIC SERVICE ALONE. `health`'s first attempt
        // hooked every route and stamped a year of `immutable` onto API JSON,
        // which is this bug pointing the other way.
        app = app.fallback_service(
            ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    header::CACHE_CONTROL,
                    cache_control_for,
                ))
                .service(serve),
        );
    }

    app.with_state(state)
}
