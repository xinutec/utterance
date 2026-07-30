//! Client activity trace: what the browser sees and the API does not.
//!
//! **Why this exists, and it is not analytics.** The per-request trace already
//! logs every API call, and for a long time that was treated as enough. It is
//! not: a press that hits a cache, a knob dragged, a control that was disabled,
//! a page that rendered wrong — none of it reaches the server, so none of it can
//! be diagnosed afterwards. This app is used by one person in another house, and
//! the only report available is "I pressed the button and nothing happened".
//!
//! The events fold into the **same** log stream as the API requests, so a
//! session reads as one timeline: `client-event kind=nav path=/studio`, then
//! `client-event kind=tap label="Render as music"`, then the
//! `GET /api/voice 400` the tap caused. That last line is the one that says what
//! went wrong, and the two before it are what say who asked for it.
//!
//! **There is no storage here.** These are logs, not data. The endpoint moves
//! the client's events into the backend log and forgets them.
//!
//! Ported from the `life` app, which has had this since 2026-07-17. The only
//! differences are that there is no per-request user to attribute to — this
//! deployment is one account — and that the gate is middleware rather than an
//! extractor, so the session is checked before a handler is reached at all.

use axum::Json;
use axum::http::StatusCode;
use serde::Deserialize;

/// One thing that happened in the client.
///
/// `kind` is `nav` for a route change, where `label` is absent, or `tap` for a
/// control, where `label` is its visible text, verbatim.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEvent {
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    /// The client's clock, in epoch milliseconds.
    ///
    /// Kept because a batch arrives all at once, so the server's receive time
    /// cannot order the events inside it and the client's can.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at: i64,
}

/// Most events accepted from one POST.
///
/// The real client batches a handful at a time; this stops a buggy or hostile
/// one turning a single request into a log flood.
const MAX_EVENTS: usize = 100;

/// Longest label kept, in characters.
///
/// Labels are verbatim UI text, so a pathological one would otherwise bloat a
/// log line. Counted in `chars` rather than bytes so a multi-byte glyph is never
/// split down the middle.
const MAX_LABEL: usize = 160;

/// `POST /api/telemetry` — fold the client's events into the log stream.
///
/// Always 204. Telemetry is best-effort: the client neither reads the response
/// nor retries, because a trace that interferes with the app it observes is
/// worse than no trace. Behind the same gate as the rest of `/api`, so this is
/// not an open log-write for anyone who finds the URL.
pub async fn record(Json(events): Json<Vec<TelemetryEvent>>) -> StatusCode {
    for e in events.into_iter().take(MAX_EVENTS) {
        let label: String = e
            .label
            .unwrap_or_default()
            .chars()
            .take(MAX_LABEL)
            .collect();
        tracing::info!(
            kind = %e.kind,
            path = %e.path,
            label = %label,
            at = e.at,
            "client-event"
        );
    }
    StatusCode::NO_CONTENT
}
