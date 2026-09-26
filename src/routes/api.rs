//! The recordings API.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::{Json, response::Response};
use serde::{Deserialize, Serialize};
use utterance_analysis::speaker::Corner;
use utterance_analysis::voiceprint::Voiceprint;

use crate::calibration::CalibrationStep;
use crate::error::AppError;
use crate::state::AppState;
use crate::store::{RecordingMeta, Role};
use crate::voice;
use utterance_mapping::mapping::{Mapping, Material};
use utterance_mapping::params::{Knob, KnobQuery};

/// Serve audio so a browser can seek in it.
///
/// An `<audio>` element only moves its playhead to a position it can fetch, and
/// without `Accept-Ranges` it cannot ask — so a seek is silently dropped and the
/// page looks broken. Both audio endpoints go through here.
fn audio_response(bytes: Vec<u8>, range: Option<&str>) -> Response {
    let total = bytes.len() as u64;
    let common = [
        (header::CONTENT_TYPE, "audio/wav".to_string()),
        // Advertised even on a full response: it is how the element learns that
        // seeking is available.
        (header::ACCEPT_RANGES, "bytes".to_string()),
    ];

    let Some((start, end)) = range.and_then(|r| parse_range(r, total)) else {
        return (common, bytes).into_response();
    };

    let slice = bytes[start as usize..=end as usize].to_vec();
    (
        StatusCode::PARTIAL_CONTENT,
        common,
        [(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{total}"),
        )],
        slice,
    )
        .into_response()
}

/// The inclusive byte range a `Range` header asks for, if we serve it.
///
/// Only the single-range `bytes=start-end` forms, which is all a media element
/// sends; anything else gets the whole file, which is correct if wasteful.
fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?;
    if spec.contains(',') || total == 0 {
        return None;
    }
    let (from, to) = spec.split_once('-')?;

    let (start, end) = match (from.trim(), to.trim()) {
        // `bytes=-500`: the last 500 bytes.
        ("", last) => {
            let len: u64 = last.parse().ok()?;
            (total.saturating_sub(len), total - 1)
        }
        (first, "") => (first.parse().ok()?, total - 1),
        (first, last) => (
            first.parse().ok()?,
            last.parse::<u64>().ok()?.min(total - 1),
        ),
    };

    (start <= end && start < total).then_some((start, end))
}

/// Run `work` on tokio's blocking pool rather than on an async worker.
///
/// Every handler that touches the store, analysis or synthesis goes through
/// here. The pod gets two async workers (its CPU limit), and two renders run
/// inline would occupy both: measured, `/healthz` took 1.4 s during two renders
/// against 1 ms idle. A panic inside `work` is resumed, failing the request as
/// it would have inline.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|e| std::panic::resume_unwind(e.into_panic()))
}

/// Query string of the endpoints that need a speaker's musical world.
#[derive(Debug, Deserialize)]
pub struct VoiceParams {
    /// Recording to derive the scale and timbre from. Absent lets
    /// `crate::voice::calibrate` choose; present is how a listener disagrees.
    #[serde(default)]
    pub calibration: Option<String>,
    /// Which mappings to hear, comma separated. `field` (the default) and
    /// `tonnetz` make a texture, `notes` makes events; a texture and `notes`
    /// can sound together.
    #[serde(default)]
    pub mapping: Option<String>,
}

/// Query string of `POST /api/recordings`.
#[derive(Debug, Deserialize)]
pub struct UploadParams {
    /// Human label for the take. Optional; the id is used when absent.
    #[serde(default)]
    pub label: Option<String>,
    /// Whether this take defines the speaker or is only material to render.
    /// Absent means material: an upload that did not say what it was for must
    /// not shape the sound world.
    #[serde(default)]
    pub role: Role,
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

/// `POST /api/recordings?label=…` — body is the raw WAV file. Analysed before
/// answering: seconds at most, which a job queue would not be worth.
pub async fn upload(
    State(app): State<AppState>,
    Query(params): Query<UploadParams>,
    body: Bytes,
) -> Result<Json<RecordingDetail>, AppError> {
    if body.is_empty() {
        return Err(AppError::BadRequest("request body was empty".into()));
    }

    blocking(move || {
        let voiceprint = utterance_analysis::analyse_wav(&body)?;
        let meta = app.store.put(
            &body,
            params.label.as_deref().unwrap_or_default(),
            &voiceprint,
            params.role,
        )?;
        tracing::info!(
            "stored {} ({:.1}s, {:.0}% voiced, {} onsets)",
            meta.id,
            meta.duration_s,
            meta.voiced_fraction * 100.0,
            meta.onset_count
        );
        Ok(Json(RecordingDetail { meta, voiceprint }))
    })
    .await
}

/// `GET /api/recordings` — every stored recording, newest first.
pub async fn list(State(app): State<AppState>) -> Result<Json<Vec<RecordingMeta>>, AppError> {
    blocking(move || Ok(Json(app.store.list()?))).await
}

/// `GET /api/recordings/{id}` — one recording with its voiceprint.
pub async fn detail(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RecordingDetail>, AppError> {
    blocking(move || {
        Ok(Json(RecordingDetail {
            meta: app.store.meta(&id)?,
            voiceprint: app.store.voiceprint(&id)?,
        }))
    })
    .await
}

/// `GET /api/recordings/{id}/audio` — the original file, for playback.
pub async fn audio(
    State(app): State<AppState>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    let bytes = blocking(move || app.store.audio(&id)).await?;
    Ok(audio_response(
        bytes,
        headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
    ))
}

/// Body of `PUT /api/recordings/{id}/role`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleBody {
    pub role: Role,
}

/// `PUT /api/recordings/{id}/role` — say what an already-stored take is for.
///
/// A take that arrived as a file or before roles existed has no other way to be
/// told. Idempotent: the body is the complete new value.
pub async fn put_role(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RoleBody>,
) -> Result<Json<RecordingMeta>, AppError> {
    blocking(move || Ok(Json(app.store.put_role(&id, body.role)?))).await
}

/// `DELETE /api/recordings/{id}`.
pub async fn delete(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Deleted>, AppError> {
    blocking(move || {
        app.store.delete(&id)?;
        Ok(Json(Deleted { id }))
    })
    .await
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    pub id: String,
}

/// One note of the speaker's derived scale, as the browser sees it: cents
/// rounded, roughness left out.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ScaleDegree {
    pub cents: f32,
    pub ratio: f32,
    /// How firmly this is a note (`utterance_mapping::tuning::Degree::depth`).
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
    /// Spectra the tone moves between, ordered dark to bright: one per
    /// calibration take that held a pitch.
    pub palette: Vec<Vec<f32>>,
    /// Spread among partials in cents, from the speaker's own pitch instability.
    pub detune_cents: f32,
    /// Which recording the scale was derived from.
    pub calibration_id: String,
    pub calibration_label: String,
    /// How many takes went into the speaker profile.
    pub takes: usize,
    /// Why the mapping asked for cannot be played in this scale, if it cannot.
    ///
    /// Here as well as on the render because the render is fetched by an
    /// `<audio>` element, which shows a failure as a broken player with no
    /// message. This is fetched by script first, so the refusal can be read.
    pub refusal: Option<String>,
}

/// Where one of this speaker's held vowels actually sat, with the step it was
/// recorded for — "ee" is what the person was asked to say.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SpeakerCorner {
    pub step: CalibrationStep,
    pub corner: Corner,
    pub f1_hz: f32,
    pub f2_hz: f32,
    /// Interquartile spread across the take, in Hz — how still the vowel was held.
    pub f1_spread_hz: f32,
    pub f2_spread_hz: f32,
    /// Frames the centre was measured over.
    pub frames: usize,
}

/// Where this speaker's vowel space actually has its corners.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SpeakerCorners {
    /// One per corner step recorded, front to back. Empty until they exist.
    pub corners: Vec<SpeakerCorner>,
}

/// `GET /api/speaker/corners` — this speaker's own vowel corners.
///
/// Not part of `/api/voice`: a chart wants them even when the takes are too
/// short to derive a scale from.
pub async fn speaker_corners(
    State(app): State<AppState>,
) -> Result<Json<SpeakerCorners>, AppError> {
    let corners = blocking(move || voice::corners(&app.store)).await?;
    Ok(Json(SpeakerCorners {
        corners: corners
            .into_iter()
            .map(|c| SpeakerCorner {
                step: c.step,
                corner: c.corner,
                f1_hz: c.measured.f1_hz,
                f2_hz: c.measured.f2_hz,
                f1_spread_hz: c.measured.f1_spread_hz,
                f2_spread_hz: c.measured.f2_spread_hz,
                frames: c.measured.frames,
            })
            .collect(),
    }))
}

/// One mapping a render may ask for, with the label and blurb a UI offers it by.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MappingChoice {
    pub name: Mapping,
    pub label: String,
    /// The material this mapping makes. Two of a kind cannot sound together, so
    /// the UI turns one off when the other is chosen.
    pub makes: Material,
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

/// `GET /api/controls` — the knobs, their ranges and what each one does. The UI
/// builds its controls from this, so a knob added in Rust needs no UI change.
pub async fn controls() -> Json<Controls> {
    Json(Controls {
        // Forwarded, not copied. The table is the wire shape.
        knobs: utterance_mapping::params::KNOBS.to_vec(),
        mappings: Mapping::ALL
            .iter()
            .map(|m| MappingChoice {
                name: *m,
                label: m.label().to_string(),
                makes: m.makes(),
                about: m.about().to_string(),
            })
            .collect(),
    })
}

/// `GET /api/voice` — the scale, timbre and tonic the speaker's takes imply.
pub async fn voice_summary(
    State(app): State<AppState>,
    Query(params): Query<VoiceParams>,
    Query(query): Query<KnobQuery>,
) -> Result<Json<VoiceSummary>, AppError> {
    let knobs = query.params();
    // Checked first, the same way the render checks it, so a name refused there
    // cannot succeed here.
    let chosen = chosen_mappings(&params)?;
    let calibrated = blocking(move || {
        voice::calibrate_with(&app.store, params.calibration.as_deref(), knobs.density)
    })
    .await?;

    // Bound as the render binds it: the scale shown must be the scale played.
    let tuning = utterance_mapping::params::bind_toward_equal(&calibrated.voice.tuning, knobs.bind);

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
        // The render's verdict, against the speaker's own scale rather than the
        // bound one: the Tonnetz binds each note after laying out its lattice,
        // so whether a plane exists does not depend on `bind`.
        refusal: refusal(&calibrated.voice.tuning, &chosen),
    }))
}

/// Points a stream is reduced to before it is sent to a browser — more than a
/// screen is wide. See [`reduce`] for how.
const STREAM_POINTS: usize = 1200;

/// A score as something to draw. The score rather than the audio, because
/// *which knob changed what* is legible there and buried in a waveform.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ScoreView {
    pub duration_s: f32,
    /// Seconds per point after reduction, so a chart can label its time axis.
    pub step_s: f32,
    /// Position on the dark-to-bright axis, per point.
    pub colour: Vec<f32>,
    /// Fraction of the tone that is breath, per point.
    pub breath: Vec<f32>,
    /// Total amplitude across every voice, per point.
    pub level: Vec<f32>,
    /// Frequency per voice per point, in Hz. Outer index is the voice.
    pub voices: Vec<Vec<f32>>,
    /// Amplitude per voice per point, indexed the same way.
    pub gains: Vec<Vec<f32>>,
    /// The scale this render is played in, in cents. Moves with `bind`.
    pub degrees: Vec<f32>,
    /// Where the consonants are, in seconds.
    pub consonants: Vec<f32>,
    /// Notes, for the mappings that emit them: `[start_s, duration_s, hz]`.
    pub events: Vec<[f32; 3]>,
}

/// `GET /api/recordings/{id}/score` — what the render at the same URL is made
/// of.
pub async fn score(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<VoiceParams>,
    Query(query): Query<KnobQuery>,
) -> Result<Json<ScoreView>, AppError> {
    let (score, tuning) = blocking(move || build_score(&app, &id, &params, &query)).await?;

    let (colour, breath, level, voices, gains, step_s) = match &score.field {
        Some(field) => {
            let frames = field.frames();
            let step_s = field.hop_s * bucket(frames) as f32;
            let level: Vec<f32> = (0..frames)
                .map(|i| field.gains.iter().map(|g| g[i]).sum::<f32>())
                .collect();
            (
                reduce(&field.colour),
                reduce(&field.breath),
                reduce(&level),
                field.voices.iter().map(|v| reduce(v)).collect(),
                field.gains.iter().map(|g| reduce(g)).collect(),
                step_s,
            )
        }
        // A note mapping has no per-frame streams: empty series, and the events.
        None => (
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0.0,
        ),
    };

    Ok(Json(ScoreView {
        duration_s: score.duration_s,
        step_s,
        colour,
        breath,
        level,
        voices,
        gains,
        degrees: tuning.degrees.iter().map(|d| d.cents).collect(),
        consonants: score.noise.iter().map(|n| n.start_s).collect(),
        events: score
            .events
            .iter()
            .map(|e| [e.start_s, e.duration_s, e.hz])
            .collect(),
    }))
}

/// Frames per output point, at least one.
fn bucket(frames: usize) -> usize {
    frames.div_ceil(STREAM_POINTS).max(1)
}

/// Reduce a per-frame series to something a chart can draw, keeping from each
/// bucket the value furthest from the middle — a brief divergence between two
/// renders is what a comparison looks for, and a mean would erase it.
fn reduce(values: &[f32]) -> Vec<f32> {
    let step = bucket(values.len());
    if step == 1 {
        return values.to_vec();
    }

    let (lo, hi) = values
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    let middle = f32::midpoint(lo, hi);

    values
        .chunks(step)
        .map(|chunk| {
            chunk.iter().copied().fold(middle, |best, v| {
                if (v - middle).abs() > (best - middle).abs() {
                    v
                } else {
                    best
                }
            })
        })
        .collect()
}

/// `GET /api/recordings/{id}/render` — this take as music, in the speaker's
/// own scale and timbre. Rendered on demand: the mapping changes too often for a
/// cached render to stay interesting.
pub async fn render(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<VoiceParams>,
    Query(query): Query<KnobQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    let bytes = blocking(move || {
        let (score, tuning) = build_score(&app, &id, &params, &query)?;
        tracing::info!(
            "rendered {} as {} notes, {} consonants and {} field voices in a {}-degree scale",
            id,
            score.events.len(),
            score.noise.len(),
            score
                .field
                .as_ref()
                .map_or(0, utterance_mapping::score::Field::voice_count),
            tuning.degrees.len(),
        );
        Ok::<_, AppError>(utterance_realisation::wav::encode(
            &utterance_realisation::synth::render(&score),
        ))
    })
    .await?;
    Ok(audio_response(
        bytes,
        headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
    ))
}

/// The score a set of parameters asks for, and the scale it is played in.
/// Shared by `render` and `score`, so the chart cannot disagree with the audio.
fn build_score(
    app: &AppState,
    id: &str,
    params: &VoiceParams,
    query: &KnobQuery,
) -> Result<
    (
        utterance_mapping::score::Score,
        utterance_mapping::tuning::Tuning,
    ),
    AppError,
> {
    let knobs = query.params();
    let calibrated =
        voice::calibrate_with(&app.store, params.calibration.as_deref(), knobs.density)?;
    let voiceprint = app.store.voiceprint(id)?;

    let chosen = chosen_mappings(params)?;

    // Two mappings making the same material cannot both be heard, so the pair
    // is refused rather than one silently losing. The same one twice is only
    // redundant.
    for (i, mapping) in chosen.iter().enumerate() {
        if let Some(rival) = chosen[i + 1..]
            .iter()
            .find(|r| *r != mapping && r.makes() == mapping.makes())
        {
            let (name, rival, makes) = (mapping.name(), rival.name(), mapping.makes().name());
            return Err(AppError::BadRequest(format!(
                "{name} and {rival} are two ways of making the same {makes} — ask for one"
            )));
        }
    }

    let tuning = utterance_mapping::params::bind_toward_equal(&calibrated.voice.tuning, knobs.bind);
    // Refused before rendering: a mapping that cannot apply still produces a
    // score, with no field, which sounds like consonants over silence.
    if let Some(why) = refusal(&calibrated.voice.tuning, &chosen) {
        return Err(AppError::Unplayable(why));
    }

    // Start from the texture mapping and lift the notes' events into it, so the
    // consonants both carry sound once. Chosen by material, not name, and the
    // clash check above guarantees at most one.
    let texture = chosen.iter().find(|m| m.makes() == Material::Texture);
    let mut score =
        texture
            .unwrap_or(&Mapping::Notes)
            .score_with(&voiceprint, &calibrated.voice, knobs);
    if texture.is_some() && chosen.contains(&Mapping::Notes) {
        score.events = Mapping::Notes
            .score_with(&voiceprint, &calibrated.voice, knobs)
            .events;
    }

    Ok((score, tuning))
}

/// The mappings a query asked for, defaulting to `field`. Blanks are dropped,
/// so `field,` means `field`; an unknown name is refused rather than silently
/// replaced by the default.
fn chosen_mappings(params: &VoiceParams) -> Result<Vec<Mapping>, AppError> {
    let asked: Vec<&str> = params
        .mapping
        .as_deref()
        .unwrap_or(Mapping::Field.name())
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();

    if asked.is_empty() {
        return Err(AppError::BadRequest("no mapping asked for".into()));
    }
    asked
        .into_iter()
        .map(|name| {
            Mapping::from_name(name).ok_or_else(|| {
                let known: Vec<&str> = Mapping::ALL.iter().map(|m| m.name()).collect();
                AppError::BadRequest(format!(
                    "no mapping called {name} — try {}, or several at once",
                    known.join(", ")
                ))
            })
        })
        .collect()
}

/// Why the mappings asked for cannot be played in this scale, if they cannot.
/// Only the lattice can fail: it needs two intervals pointing different ways,
/// where the others play whatever degrees there are.
fn refusal(tuning: &utterance_mapping::tuning::Tuning, chosen: &[Mapping]) -> Option<String> {
    if !chosen.contains(&Mapping::Tonnetz) {
        return None;
    }
    let label = Mapping::Tonnetz.label();
    utterance_mapping::lattice::Lattice::from_tuning(tuning)
        .err()
        .map(|no_plane| format!("{label} cannot be played in this scale: {no_plane}"))
}
