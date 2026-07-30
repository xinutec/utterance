//! HTTP routing table.

pub mod api;
pub mod telemetry;

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use tower_http::services::{ServeDir, ServeFile};

use crate::http_trace;
use crate::state::AppState;
use crate::webauth::{self, WebAuth};

/// Largest accepted upload.
///
/// Half a minute of 48 kHz 16-bit stereo is under 6 MB; the headroom covers a
/// long take at 96 kHz without inviting anyone to post a film. axum's 2 MB
/// default would reject a normal recording.
const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

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
        app = app.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)));
    }

    app.with_state(state)
}
