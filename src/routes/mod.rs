//! HTTP routing table.

pub mod api;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use tower_http::services::{ServeDir, ServeFile};

use crate::state::AppState;

/// Largest accepted upload.
///
/// Half a minute of 48 kHz 16-bit stereo is under 6 MB; the headroom covers a
/// long take at 96 kHz without inviting anyone to post a film. axum's 2 MB
/// default would reject a normal recording.
const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/recordings", post(api::upload).get(api::list))
        .route("/recordings/{id}", get(api::detail).delete(api::delete))
        .route("/recordings/{id}/audio", get(api::audio))
        .route("/recordings/{id}/render", get(api::render))
        .route("/voice", get(api::voice_summary))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES));

    let mut app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/api", api);

    // Serve the built Angular bundle from the same origin, falling back to
    // index.html so client-side routes resolve on reload. API-only when unset,
    // which is the dev arrangement: ng serve holds the app and proxies here.
    if let Some(dir) = state.cfg.static_dir.clone() {
        let index = dir.join("index.html");
        app = app.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)));
    }

    app.with_state(state)
}
