//! A voiceprint and a tuning become a score.
//!
//! The first mapping in the project that produces something playable, and the
//! crudest thing that could be called one. Every rule below is a decision, none
//! is forced by the measurements, and all of them are meant to be replaced:
//!
//! - **when** a note happens: at a detected onset
//! - **how long** it lasts: until the next onset
//! - **which** degree it takes: from where the vowel sat left-to-right in the
//!   speaker's own vowel space
//! - **which octave**: from where it sat top-to-bottom in that same space
//! - **how loud**: from the energy envelope at that moment
//!
//! The weak link is the first. Onsets mean *the spectrum changed here*, not *a
//! syllable began here*, and until the stress hierarchy exists the rhythm will
//! be wrong in ways that have nothing to do with this mapping's taste. That is
//! documented in `docs/roadmap.md` and is the reason to hear this before
//! polishing anything else.
//!
//! The vowel-to-pitch rule is the part worth arguing about. Mapping frontness to
//! scale degree and openness to register is legible — closed front vowels come
//! out high and bright, open back vowels low — but it is one choice out of
//! many, and it spends a two-dimensional measurement on a one-dimensional scale
//! plus an octave.

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

/// Turn a voiceprint into a score, in the world a [`Voice`] describes.
///
/// The voice comes from the speaker's calibration rather than from this take.
/// They are facts about the person, and reading them from the utterance would
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

        events.push(Event {
            start_s,
            duration_s: (next_s - start_s).clamp(MIN_NOTE_S, MAX_NOTE_S),
            hz: voice.tonic_hz * 2f32.powf(register) * degree.ratio,
            amplitude,
        });
    }

    Score {
        duration_s: vp.source.duration_s,
        timbre: voice.timbre.clone(),
        events,
    }
}

/// A score with no notes in it, for a take nothing could be read from.
fn empty(vp: &Voiceprint, voice: &Voice) -> Score {
    Score {
        duration_s: vp.source.duration_s,
        timbre: voice.timbre.clone(),
        events: Vec::new(),
    }
}

/// Which degree a normalised position picks.
///
/// Positions outside `0..1` are real — the vowel-space bounds are percentiles,
/// so a frame past the speaker's usual reach is a measurement rather than an
/// error — and this is where they stop being real, because a scale has ends.
/// Clamping is the decision; it happens once, here, rather than being smeared
/// through the measurement layers that had no business making it.
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
