//! HTTP routing table.

pub mod api;
pub mod telemetry;

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderValue, Response, header};
use axum::routing::{get, post, put};
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::http_trace;
use crate::state::AppState;
use crate::webauth::{self, WebAuth};

/// Largest accepted upload: roomy enough for a long take at 96 kHz; axum's 2 MB
/// default would reject an ordinary one.
const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

/// How long a static response may be reused without asking again.
///
/// ⚠ **`index.html` must revalidate.** Without `Cache-Control` a client may keep
/// it for days, and since it names the content-hashed bundle, a deploy stays
/// invisible. `no-cache` means "ask first"; the `ETag` makes that a cheap 304.
/// Everything else Angular emits has a hash in its name, so `immutable` is
/// honest there. Generic over the body, which is never read.
fn cache_control_for<B>(res: &Response<B>) -> Option<HeaderValue> {
    // ⚠ A 404 is not an asset: a year of `immutable` on a missing file stops the
    // client asking again. But not `!is_success()`, which would strip the
    // headers from a 304 and turn every revalidation into a full fetch.
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

/// Serve the app's page for a client-side route, and 404 anything that named a
/// file — a font answered with HTML is a silent 200. A file is a name with a
/// dot in its last segment; the heuristic beats listing the hashed assets.
fn spa(index: &str, path: &str) -> axum::response::Response {
    use axum::response::IntoResponse as _;

    if path
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
    {
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    }
    match std::fs::read_to_string(index) {
        Ok(page) => axum::response::Html(page).into_response(),
        Err(error) => {
            // A misconfigured deployment says so rather than serving a blank app.
            tracing::error!("the app's index could not be read: {error}");
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "no index").into_response()
        }
    }
}

pub fn router(state: AppState) -> Router {
    router_with(state, WebAuth::from_env().map(Arc::new))
}

/// The router with sign-in passed in rather than read from the environment, so
/// a test's gate does not leak into the others.
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

    // The API alone, so `/healthz` and the sign-in routes stay open.
    let api = match &auth {
        Some(gate) => {
            let gate = gate.clone();
            api.layer(axum::middleware::from_fn(move |request, next| {
                webauth::gate(gate.clone(), request, next)
            }))
        }
        None => api,
    };

    // Outside the gate: a later `layer` wraps earlier ones, and a refused
    // request is the one most worth a line.
    let api = api.layer(http_trace::layer());

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api", api);

    // Only when the gate is up, and traced: a sign-in that fails silently is
    // undiagnosable.
    if let Some(gate) = auth {
        app = app.merge(webauth::routes(gate).layer(http_trace::layer()));
    }

    // The built bundle from the same origin, with index.html for client routes.
    // Unset in dev, where ng serve holds the app and proxies here.
    if let Some(dir) = state.cfg.static_dir.clone() {
        let index = dir.join("index.html").to_string_lossy().into_owned();
        let serve = ServeDir::new(&dir).fallback(get(move |uri: axum::http::Uri| {
            let index = index.clone();
            async move { spa(&index, uri.path()) }
        }));
        // The header layer wraps the static service alone, not the API.
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
