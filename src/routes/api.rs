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
use crate::voice;

/// Query string of the endpoints that need a speaker's musical world.
#[derive(Debug, Deserialize)]
pub struct VoiceParams {
    /// Recording to derive the scale and timbre from.
    ///
    /// Absent means "choose one" — see `crate::voice::calibrate`. Present is how
    /// a listener disagrees with that choice, which they will, because which
    /// vowel a tuning should come from is not settled.
    #[serde(default)]
    pub calibration: Option<String>,
}

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

/// One note of the speaker's derived scale, as the browser sees it.
///
/// A wire type rather than `music_mapping::tuning::Degree` re-exported: the
/// mapping crate has no business carrying serialisation for a UI, and a scale
/// shown to a person wants the cents rounded and the roughness left out.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ScaleDegree {
    pub cents: f32,
    pub ratio: f32,
    /// How firmly this is a note rather than a technicality. See
    /// `music_mapping::tuning::Degree::depth`.
    pub depth: f32,
}

/// The musical world derived from a speaker's recordings.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct VoiceSummary {
    /// Where the music centres — this speaker's median pitch.
    pub tonic_hz: f32,
    pub degrees: Vec<ScaleDegree>,
    /// Spectra the tone moves between, ordered dark to bright.
    ///
    /// One per calibration take that held a pitch — the speaker's own vowels,
    /// which is what gives the output a timbre that moves rather than one fixed
    /// colour.
    pub palette: Vec<Vec<f32>>,
    /// Spread among partials in cents, from the speaker's own pitch instability.
    pub detune_cents: f32,
    /// Which recording the scale was derived from.
    pub calibration_id: String,
    pub calibration_label: String,
    /// How many takes went into the speaker profile.
    pub takes: usize,
}

/// `GET /api/voice` — the scale, timbre and tonic the speaker's takes imply.
pub async fn voice_summary(
    State(app): State<AppState>,
    Query(params): Query<VoiceParams>,
) -> Result<Json<VoiceSummary>, AppError> {
    let calibrated = voice::calibrate(&app.store, params.calibration.as_deref())?;
    Ok(Json(VoiceSummary {
        tonic_hz: calibrated.voice.tonic_hz,
        degrees: calibrated
            .voice
            .tuning
            .degrees
            .iter()
            .map(|d| ScaleDegree {
                cents: d.cents,
                ratio: d.ratio,
                depth: d.depth,
            })
            .collect(),
        palette: calibrated.voice.palette.clone(),
        detune_cents: calibrated.voice.detune_cents,
        calibration_id: calibrated.source.id.clone(),
        calibration_label: calibrated.source.label.clone(),
        takes: calibrated.profile.takes,
    }))
}

/// `GET /api/recordings/{id}/render` — this take as music, in the speaker's
/// own scale and timbre.
///
/// Rendered on demand rather than stored. It is a pure function of the take, the
/// calibration and the mapping, and the mapping is the thing we expect to change
/// hourly — a cached render would be stale the moment it was interesting.
pub async fn render(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<VoiceParams>,
) -> Result<Response, AppError> {
    let calibrated = voice::calibrate(&app.store, params.calibration.as_deref())?;
    let voiceprint = app.store.voiceprint(&id)?;

    let score = music_mapping::compose::compose(&voiceprint, &calibrated.voice);
    tracing::info!(
        "rendered {} as {} notes in a {}-degree scale from {}",
        id,
        score.events.len(),
        calibrated.voice.tuning.degrees.len(),
        calibrated.source.label
    );

    let bytes = music_realisation::wav::encode(&music_realisation::synth::render(&score));
    Ok(([(header::CONTENT_TYPE, "audio/wav")], bytes).into_response())
}
