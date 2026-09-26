//! How failures reach the client: a machine-readable `code` to branch on and a
//! message for a person.

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
    /// A valid request this scale cannot play — the fix is moving a setting, not
    /// the request.
    #[error("{0}")]
    Unplayable(String),
    /// Nothing in the store says who the speaker is — its own code because the
    /// next move, recording the guided vowels, can be offered as a button.
    #[error("{0}")]
    NeedsCalibration(String),
}

/// Every failure this server can name, exported by ts-rs so the browser branches
/// on a union, and a code that does not exist fails to compile there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The bytes are not audio this server can read.
    AudioUndecodable,
    /// Decoded, and there are no samples in it.
    AudioEmpty,
    /// Decoded, and too short for analysis to say anything.
    AudioTooShort,
    NotFound,
    /// A stored record that cannot be read back. Ours, not the caller's.
    RecordCorrupt,
    StorageIo,
    BadRequest,
    /// The request is fine and this scale cannot play it. See [`AppError::Unplayable`].
    Unplayable,
    /// Nothing in the store says who the speaker is. See [`AppError::NeedsCalibration`].
    NoCalibration,
    /// No session. The page turns this one into a sign-in prompt.
    NotAuthenticated,
    /// A session, for somebody this deployment does not serve.
    NotPermitted,
    /// The sign-in round trip came back with a state we did not issue, or one
    /// that has expired.
    BadLoginState,
    /// Nextcloud sent us back without the code we need to finish signing in.
    NoAuthorizationCode,
    /// The exchange with Nextcloud itself failed. Ours to diagnose, from the log.
    SignInFailed,
}

impl ErrorCode {
    /// Every code, so a test can walk them.
    pub const ALL: [ErrorCode; 14] = [
        ErrorCode::AudioUndecodable,
        ErrorCode::AudioEmpty,
        ErrorCode::AudioTooShort,
        ErrorCode::NotFound,
        ErrorCode::RecordCorrupt,
        ErrorCode::StorageIo,
        ErrorCode::BadRequest,
        ErrorCode::Unplayable,
        ErrorCode::NoCalibration,
        ErrorCode::NotAuthenticated,
        ErrorCode::NotPermitted,
        ErrorCode::BadLoginState,
        ErrorCode::NoAuthorizationCode,
        ErrorCode::SignInFailed,
    ];

    /// The wire spelling, for log lines; `tests/errors.rs` holds it to serde's.
    pub fn name(self) -> &'static str {
        match self {
            ErrorCode::AudioUndecodable => "audio_undecodable",
            ErrorCode::AudioEmpty => "audio_empty",
            ErrorCode::AudioTooShort => "audio_too_short",
            ErrorCode::NotFound => "not_found",
            ErrorCode::RecordCorrupt => "record_corrupt",
            ErrorCode::StorageIo => "storage_io",
            ErrorCode::BadRequest => "bad_request",
            ErrorCode::Unplayable => "unplayable",
            ErrorCode::NoCalibration => "no_calibration",
            ErrorCode::NotAuthenticated => "not_authenticated",
            ErrorCode::NotPermitted => "not_permitted",
            ErrorCode::BadLoginState => "bad_login_state",
            ErrorCode::NoAuthorizationCode => "no_authorization_code",
            ErrorCode::SignInFailed => "sign_in_failed",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    /// The wire spelling, so a log line can be grepped for what a client saw.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The JSON body of any failed request.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    /// Stable identifier for the failure class.
    pub code: ErrorCode,
    /// Human-readable detail. Wording is not stable; do not match on it.
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            // Unusable audio is the client's to fix: 4xx.
            AppError::Analysis(AnalysisError::Decode(_)) => {
                (StatusCode::BAD_REQUEST, ErrorCode::AudioUndecodable)
            }
            AppError::Analysis(AnalysisError::Empty) => {
                (StatusCode::BAD_REQUEST, ErrorCode::AudioEmpty)
            }
            AppError::Analysis(AnalysisError::TooShort { .. }) => {
                (StatusCode::BAD_REQUEST, ErrorCode::AudioTooShort)
            }
            AppError::Store(StoreError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, ErrorCode::NotFound)
            }
            AppError::Store(StoreError::Corrupt { .. }) => {
                (StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::RecordCorrupt)
            }
            AppError::Store(StoreError::Io { .. }) => {
                (StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::StorageIo)
            }
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, ErrorCode::BadRequest),
            AppError::Unplayable(_) => (StatusCode::UNPROCESSABLE_ENTITY, ErrorCode::Unplayable),
            AppError::NeedsCalibration(_) => (StatusCode::BAD_REQUEST, ErrorCode::NoCalibration),
        };

        // Server faults are logged here; client faults are visible to the client.
        if status.is_server_error() {
            tracing::error!("{code}: {self}");
        }

        (
            status,
            Json(ErrorBody {
                code,
                message: self.to_string(),
            }),
        )
            .into_response()
    }
}
