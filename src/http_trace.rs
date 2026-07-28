//! One log line per request, for the deployed server.
//!
//! Added after a sign-in on isis could only be confirmed by the *absence* of an
//! error: nothing recorded that requests were being served at all, so "it
//! works" and "nobody has tried it" looked identical from the logs.
//!
//! **Why the query string is not always logged.** The obvious thing to record
//! is the whole URI, and for this app that is genuinely useful — a render's
//! parameters are the interesting part of it, and two renders differ only
//! there. But `/auth/callback?code=…` carries a Nextcloud authorization code,
//! and `/login?return_to=…` echoes whatever a caller put in it. Writing either
//! into a log puts a credential somewhere it outlives its exchange, so those
//! two paths log without their query and everything else logs with it.

use axum::http::{Request, Uri};
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultOnFailure, DefaultOnResponse, TraceLayer};
use tracing::Level;

/// Paths whose query string must never reach the log.
const SECRET_QUERY: [&str; 2] = ["/auth/callback", "/login"];

/// How a request is turned into the span its line is emitted under.
///
/// A plain `fn` pointer rather than a closure, so the layer's type is nameable
/// and [`layer`] can have a return type someone can read.
type RequestSpan = fn(&Request<axum::body::Body>) -> tracing::Span;

/// The tracing layer this module builds.
pub type RequestTrace = TraceLayer<SharedClassifier<ServerErrorsAsFailures>, RequestSpan>;

/// What to record for a request's target.
///
/// The path always; the query only where it cannot carry a credential.
pub fn loggable(uri: &Uri) -> String {
    let path = uri.path();
    match uri.query() {
        Some(query) if !SECRET_QUERY.contains(&path) => format!("{path}?{query}"),
        // Marked rather than silently dropped: a bare path here would read as a
        // request that carried no parameters, which is the opposite of true.
        Some(_) => format!("{path}?<redacted>"),
        None => path.to_string(),
    }
}

/// A layer logging method, target, status and duration for each request.
///
/// Applied to the API and the sign-in routes rather than to the whole router,
/// because the static bundle is dozens of assets per page load and burying the
/// interesting lines is the same as not having them.
pub fn layer() -> RequestTrace {
    fn span(request: &Request<axum::body::Body>) -> tracing::Span {
        tracing::info_span!(
            "request",
            method = %request.method(),
            target = %loggable(request.uri()),
        )
    }

    TraceLayer::new_for_http()
        .make_span_with(span as RequestSpan)
        // At INFO on purpose: the fleet runs `RUST_LOG=info,utterance=debug`, so
        // anything tower-http emits at its default DEBUG would be filtered out
        // and this module would appear to do nothing.
        .on_response(DefaultOnResponse::new().level(Level::INFO))
        .on_failure(DefaultOnFailure::new().level(Level::WARN))
}
