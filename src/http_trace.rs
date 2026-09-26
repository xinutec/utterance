//! One log line per request, for the deployed server — otherwise "it works" and
//! "nobody tried it" look the same.
//!
//! The query is logged, since two renders differ only there — except on
//! `/auth/callback` (an authorization code) and `/login` (a caller-chosen
//! target), where a log would outlive the credential's exchange.

use axum::extract::OriginalUri;
use axum::http::{Request, Uri};
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultOnFailure, DefaultOnResponse, TraceLayer};
use tracing::Level;

/// Paths whose query string must never reach the log.
const SECRET_QUERY: [&str; 2] = ["/auth/callback", "/login"];

/// How a request becomes its span: a `fn` pointer, so [`layer`]'s type is
/// nameable.
type RequestSpan = fn(&Request<axum::body::Body>) -> tracing::Span;

/// The tracing layer this module builds.
pub type RequestTrace = TraceLayer<SharedClassifier<ServerErrorsAsFailures>, RequestSpan>;

/// What to record for a request's target: the path, and the query where it
/// cannot carry a credential.
pub fn loggable(uri: &Uri) -> String {
    let path = uri.path();
    match uri.query() {
        Some(query) if !SECRET_QUERY.contains(&path) => format!("{path}?{query}"),
        // Marked, not dropped: a bare path would claim there were no parameters.
        Some(_) => format!("{path}?<redacted>"),
        None => path.to_string(),
    }
}

/// A layer logging method, target, status and duration — on the API and sign-in
/// routes only, not the dozens of static assets per page load.
pub fn layer() -> RequestTrace {
    fn span(request: &Request<axum::body::Body>) -> tracing::Span {
        // The URI as sent: nesting strips `/api` from `request.uri()`.
        let uri = request
            .extensions()
            .get::<OriginalUri>()
            .map_or_else(|| request.uri(), |original| &original.0);
        tracing::info_span!(
            "request",
            method = %request.method(),
            target = %loggable(uri),
        )
    }

    TraceLayer::new_for_http()
        .make_span_with(span as RequestSpan)
        // At INFO on purpose: tower-http's default is DEBUG, which an
        // `info`-level filter drops, and this module would appear to do nothing.
        .on_response(DefaultOnResponse::new().level(Level::INFO))
        .on_failure(DefaultOnFailure::new().level(Level::WARN))
}
