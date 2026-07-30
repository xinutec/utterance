//! Assembling a speaker's musical world out of the takes on disk.
//!
//! The composition root's job, and the only place in the project that reaches
//! across all three layers. Everything here is wiring: which stored recording
//! serves as calibration, how the speaker profile is pooled, what gets rendered.
//! No measurement, no aesthetics.

use std::collections::BTreeMap;

use utterance_analysis::partials::Partials;
use utterance_analysis::speaker::{self, Corner, SpeakerProfile, VowelCorner};
use utterance_analysis::voiceprint::Voiceprint;
use utterance_mapping::tuning;
use utterance_mapping::voice::{self, Voice};

use crate::calibration::CalibrationStep;
use crate::error::AppError;
use crate::store::{RecordingMeta, Role, Store};

/// Frames of steady phonation a take needs before it can calibrate a voice.
///
/// Roughly three seconds. A scale derived from less is arithmetic performed on
/// noise, and it would be reported with exactly the same confidence as a good
/// one — so the bar is here rather than in the shrug of whoever reads it.
const MIN_CALIBRATION_FRAMES: usize = 300;

/// The takes that define the speaker, one per calibration step.
///
/// **Most recent wins.** A second attempt at a vowel replaces the first rather
/// than averaging with it: a step is re-recorded precisely because the earlier
/// take was bad, and a bad take that never stops counting is worse than no
/// re-record button at all. Steps are told apart by their label, which the
/// guided flow sets from the step id.
fn calibration_set(stored: Vec<(RecordingMeta, Voiceprint)>) -> Vec<(RecordingMeta, Voiceprint)> {
    let mut newest: BTreeMap<String, (RecordingMeta, Voiceprint)> = BTreeMap::new();
    for (meta, voiceprint) in stored {
        if meta.role != Role::Calibration {
            continue;
        }
        match newest.get(&meta.label) {
            Some((held, _)) if held.created_at_ms >= meta.created_at_ms => {}
            _ => {
                newest.insert(meta.label.clone(), (meta, voiceprint));
            }
        }
    }
    newest.into_values().collect()
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

/// This speaker's own vowel corners, as far as they have recorded them.
///
/// **Separate from [`calibrate`] on purpose, though both read the calibration
/// set.** Corners are measured from the guided vowels alone and need no scale,
/// no palette and no tonic — so a store whose takes are all too short to derive
/// a scale from still has corners, and asking through `calibrate` would refuse
/// to report them for a reason that has nothing to do with them. It is also what
/// a chart wants on load, where deriving a whole musical world would be work
/// nobody asked for.
///
/// An empty list is ordinary: it means the guided vowels have not been recorded.
pub fn corners(store: &Store) -> Result<Vec<MeasuredCorner>, AppError> {
    let metas = store.list()?;
    let stored: Vec<(RecordingMeta, Voiceprint)> = metas
        .into_iter()
        .filter_map(|m| store.voiceprint(&m.id).ok().map(|v| (m, v)))
        .collect();
    Ok(measure_corners(&calibration_set(stored)))
}

/// Measure every corner the calibration set has a take for.
///
/// **The vowel's identity comes from the step, not from the audio.** The take
/// recorded against the *ee* prompt is this speaker's *ee* by construction —
/// the same reasoning the guided flow already relies on, and the reason none of
/// this needs anyone to mark a recording by ear.
///
/// A corner step whose take is too short to measure is simply absent: the
/// calibration screen is where a thin take gets reported, and inventing a corner
/// out of thirty frames here would put a point on the chart that no one could
/// tell from a good one.
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
    // Front, open, back: the order the guided flow asks for them in, and the
    // order they are read round the chart. `calibration_set` yields takes keyed
    // by label, so without this the corners would arrive alphabetically.
    out.sort_by_key(|c| match c.corner {
        Corner::CloseFront => 0,
        Corner::Open => 1,
        Corner::CloseBack => 2,
    });
    out
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
/// root rather than in `utterance-analysis`. It also stands in for a decision nobody
/// has made: which vowel a speaker's tuning *should* come from is an open
/// question in `docs/roadmap.md`, and `override_id` is how a caller disagrees.
///
/// Everything else pools across every take, because vowel-space corners and
/// pitch range improve with material where a harmonic series does not.
pub fn calibrate(store: &Store, override_id: Option<&str>) -> Result<Calibrated, AppError> {
    calibrate_with(store, override_id, utterance_mapping::tuning::MIN_DEPTH)
}

/// The same, choosing how dense the derived scale is.
pub fn calibrate_with(
    store: &Store,
    override_id: Option<&str>,
    min_depth: f32,
) -> Result<Calibrated, AppError> {
    let metas = store.list()?;
    let stored: Vec<(RecordingMeta, Voiceprint)> = metas
        .into_iter()
        .filter_map(|m| store.voiceprint(&m.id).ok().map(|v| (m, v)))
        .collect();

    // **Only the takes that say they define the speaker.** A store fills up with
    // other people's singing — material to render — and pooling it here would
    // measure a vowel space, a pitch range and a timbre belonging to nobody. The
    // whole claim of the project is that *this* speaker's spectrum gives *this*
    // speaker's scale, and it is worth nothing if the spectrum is a crowd.
    let takes = calibration_set(stored);

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
        // Eligibility first, preference second. Choosing the richest scale
        // across *all* takes and checking the bar afterwards picks a take that
        // measured a lively spectrum out of a second of audio and then refuses
        // to use it — reporting no music while a perfectly good calibration take
        // sits in the store unexamined.
        None => takes
            .iter()
            .filter(|(_, v)| v.partials.frames_used >= MIN_CALIBRATION_FRAMES)
            .max_by_key(|(_, v)| {
                let degrees = tuning::from_partials_with(&v.partials, min_depth)
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

    // Every take that held a pitch contributes a spectrum to the palette, not
    // just the one the scale came from. A speaker who recorded several vowels
    // handed over several genuinely different spectra from one throat, and using
    // one of them is how the first renders came out with a tone that never
    // moved. Ordering happens in the mapping layer, by brightness.
    let palette: Vec<&Partials> = takes
        .iter()
        .map(|(_, v)| &v.partials)
        .filter(|p| p.frames_used >= MIN_CALIBRATION_FRAMES)
        .collect();

    // Jitter from the calibration take rather than from whatever was said: it is
    // a property of a throat, and reading it from an excited utterance would
    // make the timbre depend on the mood.
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
