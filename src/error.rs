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
    /// Nothing in the store says who the speaker is.
    ///
    /// Its own code because it is the one failure with an obvious next move —
    /// record the guided vowels — and a UI that knows the move should offer it
    /// as a button rather than print a sentence about it. Matching the message
    /// instead would break the moment somebody rewords it, which is exactly the
    /// drift `ErrorBody::message` warns against.
    #[error("{0}")]
    NeedsCalibration(String),
}

/// Every failure this server can name.
///
/// **One list, in Rust, because the browser branches on it.** These were
/// `&'static str` literals built in the two `match`es below and in `webauth`,
/// and read back in the frontend as bare strings — `err.code === "unplayable"`
/// sat in the compare page with nothing anywhere checking that the backend
/// still spells it that way. A code renamed on this side went on compiling on
/// both, and what reached a listener was the generic wording for a failure the
/// page knew perfectly well how to explain.
///
/// The enum crosses the wire through ts-rs, so the browser reads a union and a
/// comparison against a code that does not exist stops the build.
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

    /// The wire spelling.
    ///
    /// Restates the serde attribute, for the same reason `Mapping::name` does:
    /// the log line below wants a `&str`. `tests/errors.rs` serialises every
    /// variant and compares, so the two cannot drift apart in silence.
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
            // A recording the analyser cannot use is the client's problem to fix
            // — a different take, a longer one — so these are 4xx, not 500.
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

        // Server-side faults are logged where they happen; client-side ones are
        // already visible to whoever caused them.
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
