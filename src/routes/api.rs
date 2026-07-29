//! The recordings API.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::{Json, response::Response};
use serde::{Deserialize, Serialize};
use utterance_analysis::voiceprint::Voiceprint;

use crate::error::AppError;
use crate::state::AppState;
use crate::store::RecordingMeta;
use crate::voice;
use utterance_mapping::params::Params;

/// Serve audio so a browser can seek in it.
///
/// **Why this is not just a body with a content type.** An `<audio>` element
/// will only move its playhead to a position it can actually fetch, and without
/// `Accept-Ranges` it has no way to ask for one — so `currentTime = 27.5` is
/// silently ignored and the element stays where it was. That failure looks
/// exactly like a broken button: the page asks to jump, nothing happens, and
/// the next `timeupdate` reports zero.
///
/// Both audio endpoints go through here, because the compare page seeks in
/// renders and the studio seeks in the original recording, and neither has any
/// reason to be the one that cannot.
fn audio_response(bytes: Vec<u8>, range: Option<&str>) -> Response {
    let total = bytes.len() as u64;
    let common = [
        (header::CONTENT_TYPE, "audio/wav".to_string()),
        // Advertised even on a full response: it is how the element learns that
        // seeking is available at all.
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

/// The inclusive byte range a `Range` header asks for, if it asks for one we
/// serve.
///
/// Only the single-range `bytes=start-end` forms, which is all any media element
/// sends. A multi-range request would need a multipart body; answering `None`
/// sends the whole file instead, which is correct if wasteful and cannot be
/// heard.
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

/// The mappings a render may ask for.
///
/// Name, label, the material it makes, and what it does — in one table because
/// the render route validates against it and the UI offers it, and a UI listing
/// a mapping the route rejects is worse than no UI at all. Adding a mapping
/// means adding a row and a branch in `build_score`, and the compiler will not
/// remind you about the second.
///
/// **The third column is what a mapping competes for.** A score carries one
/// continuous field and one list of events, so two mappings making the same
/// material cannot both be heard — asking for both is refused rather than
/// silently resolved, since whichever lost would be a mapping someone asked for
/// and did not hear. Naming the material here rather than writing the clash out
/// as a rule means a fourth mapping inherits the answer.
const MAPPINGS: [(&str, &str, &str, &str); 3] = [
    (
        "field",
        "Field",
        TEXTURE,
        "Every frame sounds. A continuous texture that moves with the voice \
         rather than a sequence of notes.",
    ),
    (
        "tonnetz",
        "Lattice",
        TEXTURE,
        "The same texture, with the vowel walking a harmonic lattice built from \
         the speaker's own consonances. Chords hold while the mouth holds, and \
         change by keeping two voices and stepping one.",
    ),
    (
        "notes",
        "Notes",
        "events",
        "Discrete events at onsets. Closer to a melody, and the weaker of the \
         two — kept because comparing them is how either gets judged.",
    ),
];

/// The continuously sounding material. Named because two mappings make it.
const TEXTURE: &str = "texture";

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
    /// `utterance_mapping::params::Params::bind`.
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
    /// How far the vowel moves the harmony.
    #[serde(default)]
    pub reach: Option<f32>,
    /// How far past a boundary the mouth must go before the harmony follows.
    #[serde(default)]
    pub hold: Option<f32>,
    /// How far the third formant opens or clusters the chord.
    #[serde(default)]
    pub voicing: Option<f32>,
    /// How much the rate of spectral change stirs the texture.
    #[serde(default)]
    pub articulation: Option<f32>,
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
            hold: self.hold.unwrap_or(base.hold),
            voicing: self.voicing.unwrap_or(base.voicing),
            articulation: self.articulation.unwrap_or(base.articulation),
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

    let voiceprint = utterance_analysis::analyse_wav(&body)?;
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
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    let bytes = app.store.audio(&id)?;
    Ok(audio_response(
        bytes,
        headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
    ))
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
/// A wire type rather than `utterance_mapping::tuning::Degree` re-exported: the
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
    /// `utterance_mapping::tuning::Degree::depth`.
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
    /// Why the mapping asked for cannot be played in this scale, if it cannot.
    ///
    /// **Here rather than only on the render, because of when it is needed.**
    /// The render is fetched by an `<audio>` element, which is handed a URL and
    /// reports a failure as a broken player with no message — so a refusal that
    /// only lives there is a refusal nobody reads. This summary is fetched by
    /// script, under the same settings, before the player is pointed anywhere.
    /// The render refuses too; this is what makes the refusal legible.
    pub refusal: Option<String>,
}

/// One control the UI should offer, as the browser sees it.
///
/// A wire type rather than `utterance_mapping::params::Knob` re-exported, for the
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
    /// Mappings this knob reaches. Empty means every one of them.
    ///
    /// Sent so the UI can put away a control the mapping being played does not
    /// read. A slider that moves and changes nothing is the failure this whole
    /// table exists to prevent, and one belonging to another mapping is that
    /// failure with a longer explanation.
    pub mappings: Vec<String>,
    /// Whether to offer this one before anybody asks for it.
    ///
    /// Sent so the UI can show a handful of controls rather than all ten at
    /// equal weight. Which handful is a fact about the mapping — see
    /// `utterance_mapping::params::Knob::primary` for the rule — so it travels
    /// with the knob rather than being decided again in the browser.
    pub primary: bool,
}

/// One mapping a render may ask for.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct MappingChoice {
    pub name: String,
    pub label: String,
    /// The material this mapping makes. Two of a kind cannot sound together.
    ///
    /// Sent so the UI can turn one off when the other is chosen, rather than
    /// letting someone select a combination the render route refuses.
    pub makes: String,
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
/// added to `utterance_mapping::params::KNOBS` appears in the browser without anyone
/// editing the browser, and a range changed in the mapping cannot leave a slider
/// offering values the mapping clamps away.
pub async fn controls() -> Json<Controls> {
    Json(Controls {
        knobs: utterance_mapping::params::KNOBS
            .iter()
            .map(|k| Knob {
                name: k.name.to_string(),
                label: k.label.to_string(),
                min: k.min,
                max: k.max,
                step: k.step,
                default: k.default,
                about: k.about.to_string(),
                mappings: k.mappings.iter().map(|m| (*m).to_string()).collect(),
                primary: k.primary,
            })
            .collect(),
        mappings: MAPPINGS
            .iter()
            .map(|(name, label, makes, about)| MappingChoice {
                name: (*name).to_string(),
                label: (*label).to_string(),
                makes: (*makes).to_string(),
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
        // The same verdict the render will reach, from the same tuning, so what
        // is on screen cannot promise audio the render then refuses.
        refusal: refusal(&tuning, &mapping_names(&params)),
    }))
}

/// Points a stream is reduced to before it is sent to a browser.
///
/// A 46-second take is 4,600 frames per stream and there are a dozen streams;
/// at a screen's width that is several frames per pixel, so the wire cost buys
/// nothing anyone can see. Reduced by taking the extreme of each bucket rather
/// than the mean — a spike that survives averaging is a spike that was long, and
/// what someone comparing two renders is looking for is exactly the brief
/// divergence a mean would erase.
const STREAM_POINTS: usize = 1200;

/// A score as something to draw.
///
/// Why the score and not the audio: the question this answers is *which knob
/// changed what*, and the score is where that is legible. Two renders differing
/// in a waveform tell you they differ; two scores differing in `colour` and
/// nowhere else tell you the colour moved, which is the sentence someone
/// actually wants.
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

/// `GET /api/recordings/{id}/score` — what the render is made of.
///
/// Takes exactly the parameters `render` takes and answers about the same score,
/// so a chart drawn from this describes the audio at the matching URL rather
/// than something near it.
pub async fn score(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<VoiceParams>,
) -> Result<Json<ScoreView>, AppError> {
    let (score, tuning) = build_score(&app, &id, &params)?;

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
        // A note mapping has no per-frame streams at all. Empty series and the
        // events below is the honest shape for it, rather than a field
        // synthesised from the notes so the chart has something to draw.
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

/// Reduce a per-frame series to something a chart can draw.
///
/// Each bucket contributes the value furthest from the series' own middle, so a
/// brief excursion survives instead of being averaged into the surrounding
/// frames. Comparing two renders is looking for exactly those.
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
    let middle = (lo + hi) / 2.0;

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
/// own scale and timbre.
///
/// Rendered on demand rather than stored. It is a pure function of the take, the
/// calibration and the mapping, and the mapping is the thing we expect to change
/// hourly — a cached render would be stale the moment it was interesting.
pub async fn render(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<VoiceParams>,
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    let (score, tuning) = build_score(&app, &id, &params)?;
    tracing::info!(
        "rendered {} as {} notes, {} consonants and {} field voices in a {}-degree scale",
        id,
        score.events.len(),
        score.noise.len(),
        score.field.as_ref().map(|f| f.voice_count()).unwrap_or(0),
        tuning.degrees.len(),
    );

    let bytes = utterance_realisation::wav::encode(&utterance_realisation::synth::render(&score));
    Ok(audio_response(
        bytes,
        headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
    ))
}

/// The score a set of parameters asks for, and the scale it is played in.
///
/// Shared by `render` and `score` rather than written twice, because the whole
/// value of the second is that it describes the first. Two copies of this would
/// drift, and the way that failure presents is a chart that disagrees with the
/// audio next to it — which is worse than no chart, since the chart is what
/// someone would believe.
fn build_score(
    app: &AppState,
    id: &str,
    params: &VoiceParams,
) -> Result<
    (
        utterance_mapping::score::Score,
        utterance_mapping::tuning::Tuning,
    ),
    AppError,
> {
    let knobs = params.params();
    let calibrated =
        voice::calibrate_with(&app.store, params.calibration.as_deref(), knobs.density)?;
    let voiceprint = app.store.voiceprint(id)?;

    let names = mapping_names(params);
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
    // Two mappings making the same material cannot both be heard. Refused
    // rather than silently resolved: whichever one lost would be a mapping
    // someone asked for and did not hear, and the whole reason to keep more
    // than one is that they are compared.
    for (name, _, makes, _) in MAPPINGS {
        let rivals: Vec<&str> = MAPPINGS
            .iter()
            .filter(|(other, _, theirs, _)| *theirs == makes && *other != name)
            .map(|(other, ..)| *other)
            .collect();
        if names.contains(&name)
            && let Some(rival) = rivals.iter().find(|r| names.contains(r))
        {
            return Err(AppError::BadRequest(format!(
                "{name} and {rival} are two ways of making the same {makes} — ask for one"
            )));
        }
    }

    let tuning = utterance_mapping::params::bind_toward_equal(&calibrated.voice.tuning, knobs.bind);
    // Refused before anything is rendered. A mapping that cannot be applied
    // still produces a score — one with no field in it — and that renders to
    // consonants over silence, which is indistinguishable from a broken build.
    if let Some(why) = refusal(&tuning, &names) {
        return Err(AppError::Unplayable(why));
    }

    // Built by starting from one mapping and lifting the other's material into
    // it. Both carry the consonants, so taking them from the first and leaving
    // the second's behind is what stops the noise layer being played twice.
    let continuous = names.contains(&"field") || names.contains(&"tonnetz");
    let mut score = if names.contains(&"tonnetz") {
        utterance_mapping::tonnetz::score_with(&voiceprint, &calibrated.voice, knobs)
    } else if names.contains(&"field") {
        utterance_mapping::field::score_with(&voiceprint, &calibrated.voice, knobs)
    } else {
        utterance_mapping::compose::compose_with(&voiceprint, &calibrated.voice, knobs)
    };
    if continuous && names.contains(&"notes") {
        score.events =
            utterance_mapping::compose::compose_with(&voiceprint, &calibrated.voice, knobs).events;
    }

    Ok((score, tuning))
}

/// The mappings a query asked for, defaulting to the one someone starts with.
///
/// Trimmed and emptied of blanks, so `field,` and `field, notes` mean what they
/// look like. Shared by the render and the voice summary, which have to agree
/// about what was asked for or the summary will describe a different render.
fn mapping_names(params: &VoiceParams) -> Vec<&str> {
    params
        .mapping
        .as_deref()
        .unwrap_or("field")
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect()
}

/// Why the mappings asked for cannot be played in this scale, if they cannot.
///
/// Only the lattice can fail this way, and it is the only one that needs a
/// *shape* from the scale rather than a list of degrees: two intervals that
/// point different ways. Everything else works with whatever degrees it is
/// given, down to a scale of the tonic and the octave.
fn refusal(tuning: &utterance_mapping::tuning::Tuning, names: &[&str]) -> Option<String> {
    if !names.contains(&"tonnetz") {
        return None;
    }
    let label = MAPPINGS
        .iter()
        .find(|(name, ..)| *name == "tonnetz")
        .map_or("tonnetz", |(_, label, ..)| *label);
    utterance_mapping::lattice::Lattice::from_tuning(tuning)
        .err()
        .map(|no_plane| format!("{label} cannot be played in this scale: {no_plane}"))
}
