//! Everything about a speaker that a mapping needs, in one type: the scale, the
//! timbres, the tonic and the vowel space — the world an utterance plays in.
//!
//! One type because a derived scale is only consonant for tones with the
//! spectrum it came from; building both from one measurement makes a mismatch
//! impossible.

use utterance_analysis::partials::Partials;
use utterance_analysis::speaker::{Brightness, VowelSpace};

use crate::score::{self, Spectrum};
use crate::tuning::{self, Tuning};

/// Widest detune the speaker's own instability may produce, in cents: jitter
/// from an unsteady take runs to tens of cents, which is a chord, not a tone.
const MAX_DETUNE_CENTS: f32 = 12.0;

/// A speaker, as far as a mapping is concerned.
#[derive(Clone, Debug)]
pub struct Voice {
    /// The scale, derived from this speaker's own spectrum.
    pub tuning: Tuning,
    /// Spectra to move between, ordered dark to bright — one per calibration
    /// vowel, so the tone can travel the range the throat has.
    pub palette: Vec<Spectrum>,
    /// Spread among partials, from the speaker's own pitch instability.
    pub detune_cents: f32,
    /// The speaker's vowel-space extent, for normalising articulation.
    pub space: VowelSpace,
    /// The speaker's brightness range, for normalising tone colour. When it
    /// cannot be measured, colour holds still rather than borrowing a stream.
    pub brightness: Option<Brightness>,
    /// Where the music centres. Everything else is an interval from here.
    pub tonic_hz: f32,
}

impl Voice {
    /// Build from calibration material and a speaker profile. `tuning_from`
    /// gives the scale; `palette_from` the spectra, and should include it. `None`
    /// when the tuning spectrum is too thin for a scale.
    pub fn from_calibration(
        tuning_from: &Partials,
        palette_from: &[&Partials],
        detune_cents: f32,
        space: VowelSpace,
        brightness: Option<Brightness>,
        tonic_hz: f32,
    ) -> Option<Self> {
        Self::from_calibration_with(
            tuning_from,
            palette_from,
            detune_cents,
            space,
            brightness,
            tonic_hz,
            tuning::MIN_DEPTH,
        )
    }

    /// The same, choosing how dense the derived scale is — decided here, since a
    /// finished tuning cannot regain degrees the derivation discarded.
    #[allow(
        clippy::too_many_arguments,
        reason = "the calibration's knobs, each named; the doc above says why density is one of them"
    )]
    pub fn from_calibration_with(
        tuning_from: &Partials,
        palette_from: &[&Partials],
        detune_cents: f32,
        space: VowelSpace,
        brightness: Option<Brightness>,
        tonic_hz: f32,
        min_depth: f32,
    ) -> Option<Self> {
        let tuning = tuning::from_partials_with(tuning_from, min_depth)?;

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
            brightness,
            tonic_hz,
        })
    }
}

/// A measured harmonic series as a dense amplitude-per-harmonic list, with
/// silence where a harmonic was never found.
fn spectrum_of(partials: &Partials) -> Option<Spectrum> {
    let highest = partials.partials.iter().map(|p| p.number).max()? as usize;
    let mut spectrum = vec![0.0; highest];
    for p in &partials.partials {
        spectrum[p.number as usize - 1] = p.amplitude;
    }
    Some(spectrum)
}

/// Cycle-to-cycle pitch instability of a track, in cents: the median step
/// between consecutive voiced frames, so octave errors and gaps do not dominate.
/// Arithmetic over the voiceprint, so it lives here and needs no re-analysis.
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
