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
use music_mapping::params::Params;

/// The mappings a render may ask for.
///
/// Name, label, and what it does — in one table because the render route
/// validates against it and the UI offers it, and a UI listing a mapping the
/// route rejects is worse than no UI at all. Adding a mapping means adding a row
/// and a branch in `render`, and the compiler will not remind you about the
/// second, which is why the branch is a `match` on these names.
const MAPPINGS: [(&str, &str, &str); 2] = [
    (
        "field",
        "Field",
        "Every frame sounds. A continuous texture that moves with the voice \
         rather than a sequence of notes.",
    ),
    (
        "notes",
        "Notes",
        "Discrete events at onsets. Closer to a melody, and the weaker of the \
         two — kept because comparing them is how either gets judged.",
    ),
];

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
    /// Which mapping or mappings to hear, comma separated.
    ///
    /// `field` (the default) sounds every frame as a continuous texture;
    /// `notes` sounds discrete events at onsets; `field,notes` sounds both, so
    /// a stream of events sits over a texture. The only way to judge any of them
    /// is against the others.
    #[serde(default)]
    pub mapping: Option<String>,

    /// How far the speaker's own scale is used, 0..1. See
    /// `music_mapping::params::Params::bind`.
    #[serde(default)]
    pub bind: Option<f32>,
    /// How deep a dip must be to count as a note.
    #[serde(default)]
    pub density: Option<f32>,
    /// Voices sounding at once in the field.
    #[serde(default)]
    pub voices: Option<usize>,
    /// Scale degrees between one field voice and the next.
    #[serde(default)]
    pub spacing: Option<usize>,
    /// Octaves the field transposes across the speaker's pitch range.
    #[serde(default)]
    pub drift: Option<f32>,
    /// Octaves the root travels as the vowel moves front to back.
    #[serde(default)]
    pub reach: Option<f32>,
    /// Loudness of the consonants against the pitched material.
    #[serde(default)]
    pub consonants: Option<f32>,
}

impl VoiceParams {
    /// The mapping knobs, defaulted where the caller said nothing.
    fn params(&self) -> Params {
        let base = Params::default();
        Params {
            bind: self.bind.unwrap_or(base.bind),
            density: self.density.unwrap_or(base.density),
            voices: self.voices.unwrap_or(base.voices),
            spacing: self.spacing.unwrap_or(base.spacing),
            drift: self.drift.unwrap_or(base.drift),
            reach: self.reach.unwrap_or(base.reach),
            consonants: self.consonants.unwrap_or(base.consonants),
        }
        .sane()
    }
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

/// One control the UI should offer, as the browser sees it.
///
/// A wire type rather than `music_mapping::params::Knob` re-exported, for the
/// same reason `ScaleDegree` is one: the mapping crate carries no serialisation
/// for a UI. The numbers are copied straight from the knob table, so the two
/// cannot disagree about what a slider may offer.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Knob {
    /// Query-parameter name. Sent back on a render exactly as it arrives here.
    pub name: String,
    pub label: String,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub default: f32,
    pub about: String,
}

/// One mapping a render may ask for.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MappingChoice {
    pub name: String,
    pub label: String,
    pub about: String,
}

/// Everything a person can turn, described by the code that obeys it.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Controls {
    pub knobs: Vec<Knob>,
    pub mappings: Vec<MappingChoice>,
}

/// `GET /api/controls` — the knobs, their ranges and what each one does.
///
/// The UI builds its controls from this rather than from its own list, so a knob
/// added to `music_mapping::params::KNOBS` appears in the browser without anyone
/// editing the browser, and a range changed in the mapping cannot leave a slider
/// offering values the mapping clamps away.
pub async fn controls() -> Json<Controls> {
    Json(Controls {
        knobs: music_mapping::params::KNOBS
            .iter()
            .map(|k| Knob {
                name: k.name.to_string(),
                label: k.label.to_string(),
                min: k.min,
                max: k.max,
                step: k.step,
                default: k.default,
                about: k.about.to_string(),
            })
            .collect(),
        mappings: MAPPINGS
            .iter()
            .map(|(name, label, about)| MappingChoice {
                name: (*name).to_string(),
                label: (*label).to_string(),
                about: (*about).to_string(),
            })
            .collect(),
    })
}

/// `GET /api/voice` — the scale, timbre and tonic the speaker's takes imply.
pub async fn voice_summary(
    State(app): State<AppState>,
    Query(params): Query<VoiceParams>,
) -> Result<Json<VoiceSummary>, AppError> {
    let knobs = params.params();
    let calibrated =
        voice::calibrate_with(&app.store, params.calibration.as_deref(), knobs.density)?;

    // Bound here as well as in the mappings, because this is the scale someone
    // is shown while deciding whether they like it. Showing the derived degrees
    // beside a render that snapped them to equal temperament would make the one
    // number this project is trying to demonstrate a lie.
    let tuning = music_mapping::params::bind_toward_equal(&calibrated.voice.tuning, knobs.bind);

    Ok(Json(VoiceSummary {
        tonic_hz: calibrated.voice.tonic_hz,
        degrees: tuning
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
    let knobs = params.params();
    let calibrated =
        voice::calibrate_with(&app.store, params.calibration.as_deref(), knobs.density)?;
    let voiceprint = app.store.voiceprint(&id)?;

    let wanted = params.mapping.as_deref().unwrap_or("field");
    let names: Vec<&str> = wanted
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    if let Some(unknown) = names
        .iter()
        .find(|n| !MAPPINGS.iter().any(|(known, ..)| known == *n))
    {
        let known: Vec<&str> = MAPPINGS.iter().map(|(name, ..)| *name).collect();
        return Err(AppError::BadRequest(format!(
            "no mapping called {unknown} — try {}, or several at once",
            known.join(", ")
        )));
    }
    if names.is_empty() {
        return Err(AppError::BadRequest("no mapping asked for".into()));
    }

    // Built by starting from one mapping and lifting the other's material into
    // it. Both carry the consonants, so taking them from the first and leaving
    // the second's behind is what stops the noise layer being played twice.
    let mut score = if names.contains(&"field") {
        music_mapping::field::score_with(&voiceprint, &calibrated.voice, knobs)
    } else {
        music_mapping::compose::compose_with(&voiceprint, &calibrated.voice, knobs)
    };
    if names.contains(&"field") && names.contains(&"notes") {
        score.events =
            music_mapping::compose::compose_with(&voiceprint, &calibrated.voice, knobs).events;
    }
    tracing::info!(
        "rendered {} as {} notes, {} consonants and {} field voices in a {}-degree scale from {}",
        id,
        score.events.len(),
        score.noise.len(),
        score.field.as_ref().map(|f| f.voice_count()).unwrap_or(0),
        calibrated.voice.tuning.degrees.len(),
        calibrated.source.label
    );

    let bytes = music_realisation::wav::encode(&music_realisation::synth::render(&score));
    Ok(([(header::CONTENT_TYPE, "audio/wav")], bytes).into_response())
}
