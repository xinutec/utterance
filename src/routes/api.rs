//! The recordings API.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::{Json, response::Response};
use music_analysis::voiceprint::Voiceprint;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::state::AppState;
use crate::store::RecordingMeta;

/// Query string of `POST /api/recordings`.
#[derive(Debug, Deserialize)]
pub struct UploadParams {
    /// Human label for the take. Optional; the id is used when absent.
    #[serde(default)]
    pub label: Option<String>,
}

/// A recording and everything analysis found in it.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RecordingDetail {
    pub meta: RecordingMeta,
    pub voiceprint: Voiceprint,
}

/// `POST /api/recordings?label=…` — body is the raw WAV file.
///
/// Analysis runs synchronously. Half a minute of audio analyses in well under a
/// second, so a job queue would add a state machine and a polling endpoint to
/// save nothing anyone would notice.
pub async fn upload(
    State(app): State<AppState>,
    Query(params): Query<UploadParams>,
    body: Bytes,
) -> Result<Json<RecordingDetail>, AppError> {
    if body.is_empty() {
        return Err(AppError::BadRequest("request body was empty".into()));
    }

    let voiceprint = music_analysis::analyse_wav(&body)?;
    let meta = app.store.put(
        &body,
        params.label.as_deref().unwrap_or_default(),
        &voiceprint,
    )?;
    tracing::info!(
        "stored {} ({:.1}s, {:.0}% voiced, {} onsets)",
        meta.id,
        meta.duration_s,
        meta.voiced_fraction * 100.0,
        meta.onset_count
    );
    Ok(Json(RecordingDetail { meta, voiceprint }))
}

/// `GET /api/recordings` — every stored recording, newest first.
pub async fn list(State(app): State<AppState>) -> Result<Json<Vec<RecordingMeta>>, AppError> {
    Ok(Json(app.store.list()?))
}

/// `GET /api/recordings/{id}` — one recording with its voiceprint.
pub async fn detail(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RecordingDetail>, AppError> {
    Ok(Json(RecordingDetail {
        meta: app.store.meta(&id)?,
        voiceprint: app.store.voiceprint(&id)?,
    }))
}

/// `GET /api/recordings/{id}/audio` — the original file, for playback.
pub async fn audio(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let bytes = app.store.audio(&id)?;
    Ok(([(header::CONTENT_TYPE, "audio/wav")], bytes).into_response())
}

/// `DELETE /api/recordings/{id}`.
pub async fn delete(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Deleted>, AppError> {
    app.store.delete(&id)?;
    Ok(Json(Deleted { id }))
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    pub id: String,
}
