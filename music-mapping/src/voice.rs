//! Everything about a speaker that a mapping needs, bundled so it cannot be
//! assembled inconsistently.
//!
//! The mapping-layer counterpart to the speaker profile next door: the speaker
//! is the world, the utterance is the piece. A voice fixes the scale, the
//! timbre, the pitch it centres on and the vowel space its articulation is
//! measured against — none of which should change between two things the same
//! person said.
//!
//! **Why this is a type rather than four arguments.** A derived scale is only
//! consonant for tones carrying the spectrum it was derived from; tune to one
//! spectrum and synthesise another and the roughness minima stop lining up with
//! the notes. Building both from one [`Partials`] in one place makes that
//! impossible to get wrong, where four loose parameters would make it a matter
//! of everyone remembering.

use music_analysis::partials::Partials;
use music_analysis::speaker::VowelSpace;

use crate::tuning::{self, Tuning};

/// A speaker, as far as a mapping is concerned.
#[derive(Clone, Debug)]
pub struct Voice {
    /// The scale, derived from this speaker's own spectrum.
    pub tuning: Tuning,
    /// Relative amplitude of each harmonic — the same spectrum `tuning` came
    /// from, reduced to what a synthesiser needs.
    pub timbre: Vec<f32>,
    /// The speaker's vowel-space extent, for normalising articulation.
    pub space: VowelSpace,
    /// Where the music centres. Everything else is an interval from here.
    pub tonic_hz: f32,
}

impl Voice {
    /// Build from a calibration take's harmonic series and a speaker profile.
    ///
    /// Returns `None` when the spectrum is too thin to derive a scale from — a
    /// take that was not sustained phonation, most often. The caller has a
    /// better recording or has nothing, and either is better than a scale
    /// invented from two partials.
    pub fn from_calibration(partials: &Partials, space: VowelSpace, tonic_hz: f32) -> Option<Self> {
        let tuning = tuning::from_partials(partials)?;

        // Indexed by harmonic number, gaps filled with silence: a synthesiser
        // wants a dense list, and a harmonic the measurement never found is one
        // that should not sound.
        let highest = partials.partials.iter().map(|p| p.number).max()? as usize;
        let mut timbre = vec![0.0; highest];
        for p in &partials.partials {
            timbre[p.number as usize - 1] = p.amplitude;
        }

        Some(Voice {
            tuning,
            timbre,
            space,
            tonic_hz,
        })
    }
}
