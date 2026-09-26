//! Assembling a speaker's musical world out of the takes on disk: which take
//! calibrates, how the profile is pooled. Wiring only — the one place that
//! reaches across all three layers.

use std::collections::BTreeMap;

use utterance_analysis::partials::Partials;
use utterance_analysis::speaker::{self, Corner, SpeakerProfile, VowelCorner};
use utterance_analysis::voiceprint::Voiceprint;
use utterance_mapping::tuning;
use utterance_mapping::voice::{self, Voice};

use crate::calibration::CalibrationStep;
use crate::error::AppError;
use crate::store::{RecordingMeta, Role, Store};

/// Frames of steady phonation a take needs before it can calibrate a voice —
/// about three seconds. A scale from less is arithmetic on noise, reported with
/// the same confidence.
const MIN_CALIBRATION_FRAMES: usize = 300;

/// The takes that define the speaker, one per calibration step (by label).
/// **Most recent wins**: a step is re-recorded because the earlier take was bad.
fn calibration_set(stored: Vec<RecordingMeta>) -> Vec<RecordingMeta> {
    let mut newest: BTreeMap<String, RecordingMeta> = BTreeMap::new();
    for meta in stored {
        if meta.role != Role::Calibration {
            continue;
        }
        match newest.get(&meta.label) {
            Some(held) if held.created_at_ms >= meta.created_at_ms => {}
            _ => {
                newest.insert(meta.label.clone(), meta);
            }
        }
    }
    newest.into_values().collect()
}

/// The calibration set with each take's voiceprint — chosen from metadata first,
/// so material is never parsed.
fn calibration_takes(store: &Store) -> Result<Vec<(RecordingMeta, Voiceprint)>, AppError> {
    Ok(calibration_set(store.list()?)
        .into_iter()
        .filter_map(|m| store.voiceprint(&m.id).ok().map(|v| (m, v)))
        .collect())
}

/// A speaker's world, plus which recording it came from.
pub struct Calibrated {
    pub voice: Voice,
    pub profile: SpeakerProfile,
    /// The take the scale and timbre were derived from.
    pub source: RecordingMeta,
}

/// One corner of the vowel space, and which step reached it.
pub struct MeasuredCorner {
    pub corner: Corner,
    pub step: CalibrationStep,
    pub measured: VowelCorner,
}

/// This speaker's own vowel corners, as far as they have recorded them; empty
/// until the guided vowels exist. Separate from [`calibrate`]: corners need no
/// scale, so a store too thin for one still has them.
pub fn corners(store: &Store) -> Result<Vec<MeasuredCorner>, AppError> {
    Ok(measure_corners(&calibration_takes(store)?))
}

/// Measure every corner the calibration set has a take for. The vowel's identity
/// comes from the step, not the audio, so nobody marks anything by ear. A take
/// too short to measure is simply absent.
fn measure_corners(takes: &[(RecordingMeta, Voiceprint)]) -> Vec<MeasuredCorner> {
    let mut out: Vec<MeasuredCorner> = takes
        .iter()
        .filter_map(|(meta, voiceprint)| {
            let step = CalibrationStep::from_label(&meta.label)?;
            let corner = step.corner()?;
            Some(MeasuredCorner {
                corner,
                step,
                measured: speaker::corner(voiceprint)?,
            })
        })
        .collect();
    // Front, open, back: the order the guided flow asks, not alphabetical.
    out.sort_by_key(|c| match c.corner {
        Corner::CloseFront => 0,
        Corner::Open => 1,
        Corner::CloseBack => 2,
    });
    out
}

/// Build the current speaker's voice from the store's calibration takes.
///
/// **The scale comes from the take yielding the richest scale**, ties broken by
/// evidence. Not the most steady frames: that picks a long *ee* whose scale is
/// the fifth alone over a shorter *ah* with eight degrees. Which vowel a tuning
/// *should* come from is an open question (`docs/roadmap.md`); `override_id` is
/// how a caller disagrees. Everything else pools across every calibration take.
pub fn calibrate(store: &Store, override_id: Option<&str>) -> Result<Calibrated, AppError> {
    calibrate_with(store, override_id, utterance_mapping::tuning::MIN_DEPTH)
}

/// The same, choosing how dense the derived scale is.
pub fn calibrate_with(
    store: &Store,
    override_id: Option<&str>,
    min_depth: f32,
) -> Result<Calibrated, AppError> {
    // Only takes that say they define the speaker: other people's singing would
    // give a vowel space and a scale belonging to nobody.
    let takes = calibration_takes(store)?;

    if takes.is_empty() {
        return Err(AppError::NeedsCalibration(
            "no calibration take yet — record the guided vowels so the music has \
             a voice to be derived from"
                .into(),
        ));
    }

    let profile = speaker::profile(&takes.iter().map(|(_, v)| v).collect::<Vec<_>>());
    let space = profile.vowel_space.ok_or_else(|| {
        AppError::BadRequest(
            "not enough vowel material to place this speaker's articulation — \
             record the calibration vowels"
                .into(),
        )
    })?;
    let tonic_hz = profile
        .f0
        .ok_or_else(|| AppError::BadRequest("no pitch has been measured for this speaker".into()))?
        .median_hz;

    let (source, voiceprint) = match override_id {
        Some(id) => takes
            .iter()
            .find(|(m, _)| m.id == id)
            .ok_or_else(|| AppError::BadRequest(format!("no recording {id}")))?,
        // Eligibility first, preference second, or a rich-looking second of audio
        // is chosen and then refused while a good take goes unexamined.
        None => takes
            .iter()
            .filter(|(_, v)| v.partials.frames_used >= MIN_CALIBRATION_FRAMES)
            .max_by_key(|(_, v)| {
                let degrees = tuning::from_partials_with(&v.partials, min_depth)
                    .map_or(0, |t| t.degrees.len());
                (degrees, v.partials.frames_used)
            })
            .ok_or_else(|| {
                let best = takes
                    .iter()
                    .map(|(_, v)| v.partials.frames_used)
                    .max()
                    .unwrap_or(0);
                AppError::BadRequest(format!(
                    "no take holds a steady pitch for long enough to derive a scale — \
                     the longest managed {best} frames of {MIN_CALIBRATION_FRAMES}. \
                     Record a sustained vowel of a few seconds."
                ))
            })?,
    };

    // Every take that held a pitch joins the palette, so the tone can move
    // between the speaker's vowels.
    let palette: Vec<&Partials> = takes
        .iter()
        .map(|(_, v)| &v.partials)
        .filter(|p| p.frames_used >= MIN_CALIBRATION_FRAMES)
        .collect();

    // Jitter from the calibration take: a property of the throat, not the mood.
    let detune_cents = voice::jitter_cents(&voiceprint.pitch.hz);

    let voice = Voice::from_calibration_with(
        &voiceprint.partials,
        &palette,
        detune_cents,
        space,
        profile.brightness,
        tonic_hz,
        min_depth,
    )
    .ok_or_else(|| {
        AppError::BadRequest(
            "that take has too thin a harmonic series to derive a scale from".into(),
        )
    })?;

    Ok(Calibrated {
        voice,
        profile,
        source: source.clone(),
    })
}
