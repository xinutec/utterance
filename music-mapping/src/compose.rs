//! A voiceprint and a voice become a score.
//!
//! Every rule below is a decision, none is forced by the measurements, and all
//! of them are meant to be replaced:
//!
//! - **when** a note happens: at a detected onset
//! - **how long** it lasts: until the next onset
//! - **which** degree it takes: from where the vowel sat left-to-right in the
//!   speaker's own vowel space
//! - **which octave**: from where it sat top-to-bottom in that same space
//! - **how loud**: from the energy envelope at that moment
//! - **what colour**: from how bright the vowel was, and where it moved to
//!   during the note
//! - **how breathy**: from how periodic the voice was there
//!
//! The last two exist because the first version read four of the roughly ten
//! streams a voice emits and turned them into notes — the exact failure
//! `docs/architecture.md` warns about, a controller richer than the thing it
//! controls. Aperiodicity and formant *movement* are two more of them.
//!
//! The weak link remains the first rule. Onsets mean *the spectrum changed
//! here*, not *a syllable began here*, and until the stress hierarchy exists the
//! rhythm will be wrong in ways that have nothing to do with this mapping's
//! taste.
//!
//! **Where this stops short of resynthesis.** Colour follows the speaker's own
//! formant movement, which is articulation driving timbre — control, not
//! playback. What keeps it from sounding like speech is that those spectra are
//! applied to derived pitches at derived timings: the mouth shapes the tone, it
//! does not utter it.

use music_analysis::voiceprint::Voiceprint;

use crate::score::{Event, Score};
use crate::voice::Voice;

/// Octaves the register spans above the tonic.
///
/// Two, so the vowel space maps onto a range a listener holds as one voice
/// rather than as separate instruments at either end.
const REGISTER_OCTAVES: f32 = 2.0;

/// Longest a single note is held, in seconds.
///
/// A gap between onsets can be several seconds — a pause for breath, a silence
/// between phrases — and sustaining across one turns a rest into a drone.
const MAX_NOTE_S: f32 = 1.2;

/// Shortest note worth sounding.
///
/// Below this the attack and release overlap and the result is a click rather
/// than a pitch, so a note this brief says nothing about the tuning it came from.
const MIN_NOTE_S: f32 = 0.08;

/// How far from an onset to look for a frame that knows its vowel.
///
/// Onsets often land on the consonant that begins a syllable, where there is no
/// vowel to read yet. The following frames are where the vowel actually arrives,
/// so the search runs forward: 120 ms is long enough to cross a plosive burst
/// and short enough not to reach the next syllable.
const VOWEL_SEARCH_FRAMES: usize = 12;

/// Quietest note kept, relative to the loudest in the take.
///
/// Onsets fire in near-silence too, and a note rendered there is an artefact of
/// the detector rather than anything the speaker did.
const SILENCE_FLOOR: f32 = 0.02;

/// Aperiodicity at which a note is treated as entirely breath.
///
/// YIN's normalised difference runs from 0 for a perfectly periodic frame to
/// about 1 for noise. Voicing is decided far below this, so a frame reaching it
/// is one where the tracker found a pitch it barely believes — which is exactly
/// the breathy phonation worth hearing as noise rather than discarding.
const FULL_BREATH_APERIODICITY: f32 = 0.6;

/// Most of a note that may be noise.
///
/// A note that is all breath carries no pitch, and a piece made of them carries
/// no tuning. Reading breathiness is for texture, not for removing the thing the
/// texture is made of.
const MAX_BREATH: f32 = 0.7;

/// Turn a voiceprint into a score, in the world a [`Voice`] describes.
///
/// The voice comes from the speaker's calibration rather than from this take.
/// Those are facts about the person, and reading them from the utterance would
/// make the same sentence produce a different piece depending on how much of the
/// speaker's range it happened to use.
pub fn compose(vp: &Voiceprint, voice: &Voice) -> Score {
    // The octave duplicates the tonic, so it is not a separate choice.
    let degrees = &voice.tuning.degrees;
    let choices = &degrees[..degrees.len().saturating_sub(1)];
    if choices.is_empty() {
        return empty(vp, voice);
    }

    let loudest = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    let onsets = &vp.events.onset_frames;
    let mut events = Vec::new();

    for (n, &frame) in onsets.iter().enumerate() {
        let Some((f1, f2)) = vowel_near(vp, frame) else {
            continue;
        };
        let amplitude = amplitude_at(vp, frame, loudest);
        if amplitude < SILENCE_FLOOR {
            continue;
        }

        let (open, front) = voice.space.normalise(f1, f2);
        let degree = choices[index_of(front, choices.len())];
        // Inverted: an open vowel is the big, low end of the register, which is
        // the same direction the mouth moves.
        let register = ((1.0 - open).clamp(0.0, 1.0) * REGISTER_OCTAVES).floor();

        let start_s = frame as f32 * vp.frame.hop_s;
        let next_s = onsets
            .get(n + 1)
            .map(|&f| f as f32 * vp.frame.hop_s)
            .unwrap_or(vp.source.duration_s);
        let duration_s = (next_s - start_s).clamp(MIN_NOTE_S, MAX_NOTE_S);

        // Colour tracks the vowel across the note rather than freezing it at the
        // attack, so a syllable whose mouth moves produces a tone that moves.
        //
        // Clamped to the last frame that exists: a note running to the end of
        // the take lands one past the series, and without this the final note —
        // and any note held to the end — would silently lose its colour
        // movement while every other note kept it.
        let end_frame = (frame + (duration_s / vp.frame.hop_s) as usize)
            .min(vp.formants.f1.len().saturating_sub(1));
        let colour_from = front.clamp(0.0, 1.0);
        let colour_to = vowel_near(vp, end_frame)
            .map(|(a, b)| voice.space.normalise(a, b).1.clamp(0.0, 1.0))
            .unwrap_or(colour_from);

        events.push(Event {
            start_s,
            duration_s,
            hz: voice.tonic_hz * 2f32.powf(register) * degree.ratio,
            amplitude,
            colour_from,
            colour_to,
            breath: breath_at(vp, frame),
        });
    }

    Score {
        duration_s: vp.source.duration_s,
        palette: voice.palette.clone(),
        detune_cents: voice.detune_cents,
        events,
    }
}

/// A score with no notes in it, for a take nothing could be read from.
fn empty(vp: &Voiceprint, voice: &Voice) -> Score {
    Score {
        duration_s: vp.source.duration_s,
        palette: voice.palette.clone(),
        detune_cents: voice.detune_cents,
        events: Vec::new(),
    }
}

/// How much of a note should be breath, from how periodic the voice was.
fn breath_at(vp: &Voiceprint, frame: usize) -> f32 {
    let aperiodicity = vp
        .pitch
        .aperiodicity
        .get(frame)
        .copied()
        .unwrap_or_default();
    (aperiodicity / FULL_BREATH_APERIODICITY).clamp(0.0, 1.0) * MAX_BREATH
}

/// Which degree a normalised position picks.
///
/// Positions outside `0..1` are real — the vowel-space bounds are percentiles,
/// so a frame past the speaker's usual reach is a measurement rather than an
/// error — and this is where they stop being real, because a scale has ends.
/// Clamping is the decision; it happens once, here, rather than being smeared
/// through the measurement layers that had no business making it.
///
/// The same frontness also picks the colour above, which is a real cost worth
/// naming: two dimensions of the output move together where the voice offered
/// them separately. Untangling that needs a mapping that spends F2 once.
fn index_of(position: f32, count: usize) -> usize {
    let scaled = position * (count - 1) as f32;
    (scaled.round().max(0.0) as usize).min(count - 1)
}

/// F1 and F2 at or shortly after `frame`, if any frame there knows them.
fn vowel_near(vp: &Voiceprint, frame: usize) -> Option<(f32, f32)> {
    (frame..(frame + VOWEL_SEARCH_FRAMES).min(vp.formants.f1.len()))
        .find_map(|i| Some((vp.formants.f1[i]?, vp.formants.f2[i]?)))
}

/// Loudness at a frame, relative to the loudest moment in the take.
///
/// From dBFS to a linear 0..1 by way of the take's own peak, so a quietly
/// recorded take produces the same dynamics as a loud one — the shape of the
/// envelope is the measurement, not the level it was recorded at.
fn amplitude_at(vp: &Voiceprint, frame: usize, loudest_db: f32) -> f32 {
    let db = vp.rms_db.get(frame).copied().unwrap_or(f32::NEG_INFINITY);
    10f32.powf((db - loudest_db) / 20.0)
}
