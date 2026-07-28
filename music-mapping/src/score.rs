//! The score: the artefact between mapping and realisation.
//!
//! The second stable interface in the project, alongside the voiceprint, and it
//! earns its keep the same way — realisation can be rewritten without touching a
//! mapping, and a mapping can be replaced without touching a synthesiser.
//!
//! **Frequencies are absolute, in hertz.** No degrees, no scale, no key. This is
//! the mirror image of the rule that keeps analysis from knowing what a scale is:
//! realisation must not know either, or the choice of tuning leaks into the
//! synthesiser and the two stop being separable. By the time a score exists,
//! every musical decision has already been made.
//!
//! **What a score carries is the ceiling on how the music can sound.** The first
//! version held four numbers per note and one fixed spectrum for the whole piece,
//! and no amount of synthesiser craft could get past that: a spectrum that cannot
//! change produces a tone that does not move, which is the dead-organ sound of
//! every naive additive synthesiser. Widening this interface is therefore how the
//! output gets richer, not tinkering downstream of it.

use serde::{Deserialize, Serialize};

/// Relative amplitude per harmonic, starting at the fundamental.
pub type Spectrum = Vec<f32>;

/// One sounded note.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub start_s: f32,
    pub duration_s: f32,
    /// Absolute pitch. Whatever tuning produced it is already resolved.
    pub hz: f32,
    /// Relative loudness, 0..1.
    pub amplitude: f32,
    /// Where this note starts on the palette's dark-to-bright axis, 0..1.
    pub colour_from: f32,
    /// Where it has arrived by the end. Interpolated across the note.
    ///
    /// Two values rather than one because a spectrum that holds still is the
    /// whole problem this interface was widened to fix. A note whose colour
    /// moves is the difference between a tone and a drone.
    pub colour_to: f32,
    /// Fraction of this note's energy that is breath rather than partials, 0..1.
    ///
    /// Every real sound has a noise component, and its absence is most of what
    /// makes pure additive synthesis sound sterile. Carried per note because the
    /// speaker's own breathiness varies through an utterance.
    pub breath: f32,
}

/// A stretch of noise: a consonant, sounded.
///
/// Separate from [`Event`] rather than a flag on it because the two are not
/// variations of one thing. A note has a pitch and a place in a scale; this has
/// neither, and never should — a consonant is not a note played badly, it is a
/// different kind of sound with its own timing, and speech has more of them than
/// it has vowels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoiseEvent {
    pub start_s: f32,
    pub duration_s: f32,
    /// Centre of the noise band, in Hz — where the speaker put the energy.
    pub centre_hz: f32,
    /// Width of that band, in Hz.
    ///
    /// Narrow reads as a whistle or a hiss with a pitch to it; wide reads as
    /// air. The speaker's own measured flatness decides which.
    pub bandwidth_hz: f32,
    /// Relative loudness, 0..1.
    pub amplitude: f32,
}

/// Everything needed to render a piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub duration_s: f32,
    /// Spectra the colour axis interpolates between, ordered dark to bright.
    ///
    /// Carried in the score rather than chosen by the synthesiser, because a
    /// derived tuning is only consonant for tones that actually have the
    /// spectrum it was derived from. Tune to one spectrum and play another and
    /// the roughness minima no longer line up with the notes — the scale keeps
    /// its numbers and loses its justification.
    ///
    /// Ordered by spectral centroid so that `colour` means *brightness*, which
    /// is a thing a listener can hear moving. The ordering is a reduction: three
    /// measured vowels are a two-dimensional space and this walks a line through
    /// it, chosen because one axis a listener can name beats two they cannot.
    pub palette: Vec<Spectrum>,
    /// Spread among partials in cents — how far each is pulled off its exact
    /// harmonic.
    ///
    /// Perfectly locked partials are what a computer produces and nothing else
    /// does. A voice's own cycle-to-cycle instability is where this comes from,
    /// so the liveliness is the speaker's rather than a synthesiser preset's.
    pub detune_cents: f32,
    /// Ascending by start time.
    pub events: Vec<Event>,
    /// The consonants, ascending by start time.
    ///
    /// A second stream rather than more notes. In ordinary speech there are more
    /// of these than there are voiced stretches — the first version of this
    /// project discarded every one of them, which is most of what made the
    /// output sound like a reduction of a voice rather than a use of it.
    pub noise: Vec<NoiseEvent>,
}

impl Score {
    /// The spectrum at position `colour` on the palette's axis.
    ///
    /// Lives here rather than in the synthesiser because it defines what the
    /// colour numbers above *mean*, and a renderer that interpolated differently
    /// would be playing a different score than the one written.
    pub fn spectrum_at(&self, colour: f32) -> Spectrum {
        match self.palette.len() {
            0 => Vec::new(),
            1 => self.palette[0].clone(),
            n => {
                let position = colour.clamp(0.0, 1.0) * (n - 1) as f32;
                let lower = (position.floor() as usize).min(n - 2);
                let blend = position - lower as f32;
                blend_spectra(&self.palette[lower], &self.palette[lower + 1], blend)
            }
        }
    }
}

/// Linear blend of two spectra, padded to the longer of the two.
///
/// Padding rather than truncating: a partial present in one spectrum and absent
/// from the other should fade in, not vanish at the midpoint.
fn blend_spectra(a: &[f32], b: &[f32], t: f32) -> Spectrum {
    let length = a.len().max(b.len());
    (0..length)
        .map(|i| {
            let low = a.get(i).copied().unwrap_or(0.0);
            let high = b.get(i).copied().unwrap_or(0.0);
            low + (high - low) * t
        })
        .collect()
}

/// Order spectra dark to bright, by spectral centroid.
///
/// The centroid — the amplitude-weighted mean harmonic number — is the standard
/// correlate of perceived brightness, and using it means the palette's axis is
/// something a listener can follow rather than an arbitrary ordering of vowels.
pub fn order_by_brightness(mut spectra: Vec<Spectrum>) -> Vec<Spectrum> {
    spectra.sort_by(|a, b| centroid(a).total_cmp(&centroid(b)));
    spectra
}

/// Amplitude-weighted mean harmonic number of a spectrum.
pub fn centroid(spectrum: &[f32]) -> f32 {
    let total: f32 = spectrum.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    spectrum
        .iter()
        .enumerate()
        .map(|(i, a)| (i + 1) as f32 * a)
        .sum::<f32>()
        / total
}
