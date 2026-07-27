//! Everything about a speaker that a mapping needs, bundled so it cannot be
//! assembled inconsistently.
//!
//! The mapping-layer counterpart to the speaker profile next door: the speaker
//! is the world, the utterance is the piece. A voice fixes the scale, the
//! timbres, the pitch it centres on and the vowel space its articulation is
//! measured against — none of which should change between two things the same
//! person said.
//!
//! **Why this is a type rather than five arguments.** A derived scale is only
//! consonant for tones carrying the spectrum it was derived from; tune to one
//! spectrum and synthesise another and the roughness minima stop lining up with
//! the notes. Building both from the same measurement in one place makes that
//! impossible to get wrong, where loose parameters would make it a matter of
//! everyone remembering.

use music_analysis::partials::Partials;
use music_analysis::speaker::VowelSpace;

use crate::score::{self, Spectrum};
use crate::tuning::{self, Tuning};

/// Widest detune the speaker's own instability is allowed to produce, in cents.
///
/// A ceiling rather than a target. Jitter measured on a take that was not
/// actually steady runs to tens of cents, and partials pulled that far apart
/// stop being one tone and start being a chord nobody wrote.
const MAX_DETUNE_CENTS: f32 = 12.0;

/// A speaker, as far as a mapping is concerned.
#[derive(Clone, Debug)]
pub struct Voice {
    /// The scale, derived from this speaker's own spectrum.
    pub tuning: Tuning,
    /// Spectra to move between, ordered dark to bright.
    ///
    /// Several rather than one, and this is the difference between a tone that
    /// evolves and a tone that sits still. A speaker who recorded *ah*, *ee* and
    /// *oo* has handed over three genuinely different spectra from one throat;
    /// using one of them and discarding the others throws away most of the
    /// timbral range they actually have.
    pub palette: Vec<Spectrum>,
    /// Spread among partials, from the speaker's own pitch instability.
    pub detune_cents: f32,
    /// The speaker's vowel-space extent, for normalising articulation.
    pub space: VowelSpace,
    /// Where the music centres. Everything else is an interval from here.
    pub tonic_hz: f32,
}

impl Voice {
    /// Build from calibration material and a speaker profile.
    ///
    /// `tuning_from` is the take the scale is derived from; `palette_from`
    /// supplies the spectra to move between, and should include it. Returns
    /// `None` when the tuning spectrum is too thin to derive a scale from — the
    /// caller has better material or has none, and either beats a scale invented
    /// from two partials.
    pub fn from_calibration(
        tuning_from: &Partials,
        palette_from: &[&Partials],
        detune_cents: f32,
        space: VowelSpace,
        tonic_hz: f32,
    ) -> Option<Self> {
        let tuning = tuning::from_partials(tuning_from)?;

        let mut palette: Vec<Spectrum> = palette_from
            .iter()
            .filter_map(|p| spectrum_of(p))
            .filter(|s| s.iter().any(|&a| a > 0.0))
            .collect();
        if palette.is_empty() {
            palette.push(spectrum_of(tuning_from)?);
        }

        Some(Voice {
            tuning,
            palette: score::order_by_brightness(palette),
            detune_cents: detune_cents.clamp(0.0, MAX_DETUNE_CENTS),
            space,
            tonic_hz,
        })
    }
}

/// A measured harmonic series as a dense amplitude-per-harmonic list.
///
/// Gaps are filled with silence rather than interpolated: a harmonic the
/// measurement never found is one that should not sound, and inventing a level
/// for it would put energy where the speaker's vocal tract put none.
fn spectrum_of(partials: &Partials) -> Option<Spectrum> {
    let highest = partials.partials.iter().map(|p| p.number).max()? as usize;
    let mut spectrum = vec![0.0; highest];
    for p in &partials.partials {
        spectrum[p.number as usize - 1] = p.amplitude;
    }
    Some(spectrum)
}

/// Cycle-to-cycle pitch instability of a track, in cents.
///
/// The median step between consecutive voiced frames — a speaker's jitter, near
/// enough. Median rather than mean because a step across an unvoiced gap or an
/// octave error is a different phenomenon entirely, and either would dominate an
/// average.
///
/// Lives in mapping rather than in the analysis crate on purpose: it is
/// arithmetic over a measurement already published in the voiceprint, not a new
/// measurement, so deriving it here costs nobody a re-analysis of every take.
pub fn jitter_cents(hz: &[Option<f32>]) -> f32 {
    let mut steps: Vec<f32> = hz
        .windows(2)
        .filter_map(|w| match (w[0], w[1]) {
            (Some(a), Some(b)) if a > 0.0 && b > 0.0 => Some((1200.0 * (b / a).log2()).abs()),
            _ => None,
        })
        .collect();

    if steps.is_empty() {
        return 0.0;
    }
    steps.sort_by(f32::total_cmp);
    steps[steps.len() / 2]
}
