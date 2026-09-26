//! Client activity trace: what the browser sees and the API does not.
//!
//! Not analytics. A cached press, a dragged knob, a disabled control never reach
//! the server, and the only report from the other house is "I pressed it and
//! nothing happened". The events join the same log as the API requests, so a
//! session reads as one timeline — the tap, then the `GET /api/voice 400` it
//! caused. Nothing is stored; the events are logged and forgotten.

use axum::Json;
use axum::http::StatusCode;
use serde::Deserialize;

/// One thing that happened in the client: `nav` for a route change, or `tap`
/// with the control's visible text as `label`.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEvent {
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    /// The client's clock, in epoch milliseconds — a batch arrives at once, so
    /// only the client's times order it.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at: i64,
}

/// Most events accepted from one POST, so one request cannot flood the log.
const MAX_EVENTS: usize = 100;

/// Longest label kept, in characters (never splitting a glyph).
const MAX_LABEL: usize = 160;

/// Format characters that are invisible or reorder what is displayed, which
/// `char::is_control` misses: zero-width characters, and bidi overrides that
/// make a log line display something other than it says (Trojan Source, aimed
/// at the log). A deny-list rather than a Unicode tables crate.
fn is_deceptive_format(c: char) -> bool {
    matches!(c,
        '\u{00ad}'
        | '\u{200b}'..='\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{2069}'
        | '\u{feff}'
    )
}

/// Flatten a client-supplied label to a single harmless log field.
///
/// **The endpoint's security boundary**: a newline in a label forges whole log
/// lines. Control and deceptive format characters become spaces, whitespace runs
/// (including U+2028/2029) collapse, and the result is capped. Public so
/// `tests/telemetry.rs` can attack it directly.
pub fn one_line(label: &str, max: usize) -> String {
    let unbroken: String = label
        .chars()
        .map(|c| {
            if c.is_control() || is_deceptive_format(c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    unbroken
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// `POST /api/telemetry` — fold the client's events into the log. Always 204:
/// best-effort, never retried. Behind the same gate as the rest of `/api`.
pub async fn record(Json(events): Json<Vec<TelemetryEvent>>) -> StatusCode {
    for e in events.into_iter().take(MAX_EVENTS) {
        let label = one_line(&e.label.unwrap_or_default(), MAX_LABEL);
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
