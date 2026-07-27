//! Assembling a speaker's musical world out of the takes on disk.
//!
//! The composition root's job, and the only place in the project that reaches
//! across all three layers. Everything here is wiring: which stored recording
//! serves as calibration, how the speaker profile is pooled, what gets rendered.
//! No measurement, no aesthetics.

use music_analysis::speaker::{self, SpeakerProfile};
use music_analysis::voiceprint::Voiceprint;
use music_mapping::tuning;
use music_mapping::voice::Voice;

use crate::error::AppError;
use crate::store::{RecordingMeta, Store};

/// Frames of steady phonation a take needs before it can calibrate a voice.
///
/// Roughly three seconds. A scale derived from less is arithmetic performed on
/// noise, and it would be reported with exactly the same confidence as a good
/// one — so the bar is here rather than in the shrug of whoever reads it.
const MIN_CALIBRATION_FRAMES: usize = 300;

/// A speaker's world, plus which recording it came from.
pub struct Calibrated {
    pub voice: Voice,
    pub profile: SpeakerProfile,
    /// The take the scale and timbre were derived from.
    pub source: RecordingMeta,
}

/// Build the current speaker's voice from everything in the store.
///
/// **How the calibration take is chosen: the one that yields the richest scale**,
/// ties broken by how much evidence it was measured over.
///
/// The obvious criterion — most steady frames — was tried first and is wrong in
/// a way worth recording. On the first real calibration set it picked an eleven
/// second *ee*, beautifully measured, whose spectrum yields a scale of the fifth
/// and nothing else; a five second *ah* in the same session yields eight degrees
/// close to just intonation. A calibration take exists to define a musical world,
/// and one defining a world with two notes in it has failed at that job however
/// well it was measured.
///
/// This is a choice, not a measurement, which is why it lives in the composition
/// root rather than in `music-analysis`. It also stands in for a decision nobody
/// has made: which vowel a speaker's tuning *should* come from is an open
/// question in `docs/roadmap.md`, and `override_id` is how a caller disagrees.
///
/// Everything else pools across every take, because vowel-space corners and
/// pitch range improve with material where a harmonic series does not.
pub fn calibrate(store: &Store, override_id: Option<&str>) -> Result<Calibrated, AppError> {
    let metas = store.list()?;
    let takes: Vec<(RecordingMeta, Voiceprint)> = metas
        .into_iter()
        .filter_map(|m| store.voiceprint(&m.id).ok().map(|v| (m, v)))
        .collect();

    if takes.is_empty() {
        return Err(AppError::BadRequest(
            "nothing has been recorded yet — record a calibration take first".into(),
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
        // Eligibility first, preference second. Choosing the richest scale
        // across *all* takes and checking the bar afterwards picks a take that
        // measured a lively spectrum out of a second of audio and then refuses
        // to use it — reporting no music while a perfectly good calibration take
        // sits in the store unexamined.
        None => takes
            .iter()
            .filter(|(_, v)| v.partials.frames_used >= MIN_CALIBRATION_FRAMES)
            .max_by_key(|(_, v)| {
                let degrees = tuning::from_partials(&v.partials)
                    .map(|t| t.degrees.len())
                    .unwrap_or(0);
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

    let voice =
        Voice::from_calibration(&voiceprint.partials, space, tonic_hz).ok_or_else(|| {
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
