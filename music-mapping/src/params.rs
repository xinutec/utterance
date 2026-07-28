//! The knobs.
//!
//! Every number here was a constant somewhere in this crate, chosen by whoever
//! wrote the mapping and documented as arguable. They are gathered into one type
//! because the constants were never the point: the mapping layer exists to be
//! swept and compared by ear, and a value buried in a `const` can only be
//! changed by editing, rebuilding and re-rendering.
//!
//! **Why these live in mapping.** A control over how the music sounds is
//! aesthetic, so it belongs here and never in analysis — a knob in analysis
//! would invalidate every stored voiceprint each time it moved, where one here
//! is swept against a fixed voiceprint and heard immediately. That is recorded
//! as a decision in `docs/roadmap.md`.
//!
//! Defaults reproduce what the mapping did before it was parameterised, so
//! taking none of them changes nothing.

use crate::tuning::{Degree, Tuning};

/// How the voice binds, and what it drives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// How far the speaker's own scale is used, 0..1.
    ///
    /// **The convention-to-speaker axis**, and the longest-standing open
    /// question in the project. At 1 the degrees are exactly where this voice's
    /// spectrum puts them; at 0 they snap to twelve-tone equal temperament; in
    /// between they are interpolated in cents.
    ///
    /// The reason it exists rather than being decided: nobody knows where on
    /// this axis the music is, and it is not a thing anyone can settle by
    /// argument. It converts a question into something you listen to.
    pub bind: f32,
    /// How deep a dip in the roughness curve must be to count as a note.
    ///
    /// Raise it for a handful of very stable intervals, lower it for a dense
    /// microtonal set. The same speaker's *ah* gave eight degrees and their *ee*
    /// gave three, and part of that spread is this number rather than the voice.
    pub density: f32,
    /// How many voices sound at once in the field mapping.
    pub voices: usize,
    /// Scale degrees between one field voice and the next.
    pub spacing: usize,
    /// Octaves the whole field transposes across the speaker's pitch range.
    ///
    /// At 0 the prosody is discarded and the field sits still; at 1 it follows
    /// the speaker's pitch closely enough to read as a parallel melody, which is
    /// the naive mapping this project exists to avoid. The default is deliberately
    /// nearer the first.
    pub drift: f32,
    /// Octaves the root travels as the vowel moves front to back.
    pub reach: f32,
    /// How loud the consonants are against the pitched material, 0..1.
    ///
    /// At 0 they are silent, which is what every version of this project did
    /// before they were measured at all.
    pub consonants: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            bind: 1.0,
            density: crate::tuning::MIN_DEPTH,
            voices: 5,
            spacing: 2,
            drift: 0.25,
            reach: 1.0,
            consonants: 1.0,
        }
    }
}

impl Params {
    /// Clamp everything into a range that produces sound rather than an error.
    ///
    /// Called once where the values arrive rather than checked at each use: a
    /// knob that arrives out of range is someone exploring, not a bug, and the
    /// useful response is the nearest thing that works.
    pub fn sane(self) -> Self {
        Params {
            bind: self.bind.clamp(0.0, 1.0),
            density: self.density.clamp(0.0005, 0.5),
            voices: self.voices.clamp(1, 12),
            spacing: self.spacing.clamp(1, 6),
            drift: self.drift.clamp(0.0, 2.0),
            reach: self.reach.clamp(0.0, 3.0),
            consonants: self.consonants.clamp(0.0, 2.0),
        }
    }
}

/// Cents in an equal-tempered semitone.
const SEMITONE_CENTS: f32 = 100.0;

/// Pull a tuning toward equal temperament by `1 - bind`.
///
/// Interpolating in cents rather than in frequency ratio, because cents are
/// where the perceptual midpoint is: halfway between a just third at 386 and a
/// tempered one at 400 is 393, which is what a listener hears as halfway.
///
/// At `bind = 1` this returns the scale untouched. At 0 every degree lands on a
/// tempered note — which usually means the scale collapses to fewer degrees than
/// it had, since two neighbours can snap to the same place. That is honest
/// rather than a defect: it is what conventional tuning does to a spectrum that
/// did not ask for it.
pub fn bind_toward_equal(tuning: &Tuning, bind: f32) -> Tuning {
    if bind >= 1.0 {
        return tuning.clone();
    }

    let mut degrees: Vec<Degree> = tuning
        .degrees
        .iter()
        .map(|d| {
            let tempered = (d.cents / SEMITONE_CENTS).round() * SEMITONE_CENTS;
            let cents = tempered + (d.cents - tempered) * bind;
            Degree {
                cents,
                ratio: crate::tuning::cents_to_ratio(cents),
                ..*d
            }
        })
        .collect();

    // Two degrees that snapped to the same tempered note are now one note played
    // twice. Keeping both would silently double a voice in the field and change
    // the balance for a reason nothing reports.
    degrees.dedup_by(|a, b| (a.cents - b.cents).abs() < 1.0);

    Tuning {
        degrees,
        curve: tuning.curve.clone(),
    }
}
