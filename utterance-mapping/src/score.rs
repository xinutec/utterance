//! The score: the artefact between mapping and realisation, and the second
//! stable interface beside the voiceprint.
//!
//! **Frequencies are absolute, in hertz** — no degrees, no scale — so tuning
//! cannot leak into the synthesiser. By the time a score exists, every musical
//! decision is made. **What a score carries is the ceiling on how the music can
//! sound**: widening it is how the output gets richer.

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
    /// Where it has arrived by the end, interpolated across the note: a colour
    /// that moves is the difference between a tone and a drone.
    pub colour_to: f32,
    /// Fraction of this note's energy that is breath, 0..1 — per note, because
    /// the speaker's breathiness varies.
    pub breath: f32,
}

/// A stretch of noise: a consonant, sounded. Not a flag on [`Event`]: a
/// consonant has no pitch and no place in a scale, and its own timing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoiseEvent {
    pub start_s: f32,
    pub duration_s: f32,
    /// Centre of the noise band, in Hz — where the speaker put the energy.
    pub centre_hz: f32,
    /// Width of that band, in Hz: narrow whistles, wide is air.
    pub bandwidth_hz: f32,
    /// Relative loudness, 0..1.
    pub amplitude: f32,
}

/// A continuously sounding field: the music as parameter streams rather than a
/// list of events.
///
/// A note is a quantiser — one value for its whole span — so a few dozen notes
/// keep a few per cent of what thousands of frames measured. Here every frame
/// contributes, silence is a quiet field rather than an absent one, and onsets'
/// weakness stops mattering because nothing asks for them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Seconds between frames of every series below.
    pub hop_s: f32,
    /// Frequency per voice per frame, in Hz. Outer index is the voice.
    pub voices: Vec<Vec<f32>>,
    /// Amplitude per voice per frame, 0..1, indexed the same way — separate, so
    /// a voice can fade without moving and move without changing level.
    pub gains: Vec<Vec<f32>>,
    /// Position on the palette's dark-to-bright axis per frame, shared by every
    /// voice.
    pub colour: Vec<f32>,
    /// Noise fraction per frame, shared by every voice.
    pub breath: Vec<f32>,
}

impl Field {
    /// How many frames every series here holds.
    pub fn frames(&self) -> usize {
        self.colour.len()
    }

    /// How many voices sound at once.
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }
}

/// Everything needed to render a piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub duration_s: f32,
    /// Spectra the colour axis interpolates between, ordered dark to bright.
    ///
    /// In the score, not chosen by the synthesiser: a derived tuning is only
    /// consonant for tones with the spectrum it was derived from. Ordered by
    /// centroid so `colour` means brightness — one axis a listener can name.
    pub palette: Vec<Spectrum>,
    /// Spread among partials in cents, from the speaker's own pitch instability:
    /// perfectly locked partials sound machine-made.
    pub detune_cents: f32,
    /// The continuously sounding part, where there is one. Rendered alongside
    /// [`Score::events`].
    pub field: Option<Field>,
    /// Discrete notes, for mappings that produce them. Ascending by start time.
    pub events: Vec<Event>,
    /// The consonants, ascending by start time.
    pub noise: Vec<NoiseEvent>,
}

impl Score {
    /// The spectrum at position `colour` on the palette's axis. Here rather than
    /// in the synthesiser because it defines what `colour` means.
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

/// Linear blend of two spectra, padded so a partial in only one fades in rather
/// than vanishing at the midpoint.
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

/// Order spectra dark to bright by spectral centroid, the standard correlate of
/// perceived brightness.
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
