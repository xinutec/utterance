//! How failures reach the client.
//!
//! Every error carries a machine-readable `code` alongside its message. The
//! frontend branches on the code; the message is for a person. Without the code
//! the UI ends up matching on prose, which breaks the moment the wording changes.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utterance_analysis::AnalysisError;

use crate::store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Analysis(#[from] AnalysisError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    BadRequest(String),
    /// A request that makes sense and asks for something this scale cannot give.
    ///
    /// Separate from `BadRequest` because nothing about it is a mistake: the
    /// mapping exists, the knob is inside its published range, and the pair
    /// simply has no answer for this speaker. The client's move is to change a
    /// setting rather than to fix a malformed request, and the code says which.
    #[error("{0}")]
    Unplayable(String),
}

/// The JSON body of any failed request.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    /// Stable identifier for the failure class.
    pub code: String,
    /// Human-readable detail. Wording is not stable; do not match on it.
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            // A recording the analyser cannot use is the client's problem to fix
            // — a different take, a longer one — so these are 4xx, not 500.
            AppError::Analysis(AnalysisError::Decode(_)) => {
                (StatusCode::BAD_REQUEST, "audio_undecodable")
            }
            AppError::Analysis(AnalysisError::Empty) => (StatusCode::BAD_REQUEST, "audio_empty"),
            AppError::Analysis(AnalysisError::TooShort { .. }) => {
                (StatusCode::BAD_REQUEST, "audio_too_short")
            }
            AppError::Store(StoreError::NotFound(_)) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Store(StoreError::Corrupt { .. }) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "record_corrupt")
            }
            AppError::Store(StoreError::Io { .. }) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "storage_io")
            }
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Unplayable(_) => (StatusCode::UNPROCESSABLE_ENTITY, "unplayable"),
        };

        // Server-side faults are logged where they happen; client-side ones are
        // already visible to whoever caused them.
        if status.is_server_error() {
            tracing::error!("{code}: {self}");
        }

        (
            status,
            Json(ErrorBody {
                code: code.to_string(),
                message: self.to_string(),
            }),
        )
            .into_response()
    }
}
