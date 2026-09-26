//! The shape of the noise in a voice.
//!
//! Nearly three quarters of speech carries no fundamental, and every other
//! measurement gates on voicing — yet nobody's *s* sounds like anyone else's.
//! This characterises the noise rather than classifying phones, in three numbers
//! per frame, voiced or not:
//!
//! - **centroid** — where the energy sits: *s* against *sh* against *f*.
//! - **flatness** — how noise-like, 0 for a tone to 1 for white noise.
//! - **tilt** — how fast the spectrum falls, in dB per octave: shallow for a
//!   pressed voice, steep for a breathy one, independent of the centroid.

use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::frame::SPECTRAL_WINDOW;
use crate::resample::ANALYSIS_RATE;

/// Lowest frequency the measures look at. Below it, room rumble, proximity boost
/// and vowel tails dominate — measured across everything, unvoiced speech read
/// as tonal with a 153 Hz centroid. Fricative energy lives from about 2 kHz up.
pub const NOISE_BAND_LOW_HZ: f32 = 300.0;

/// Highest frequency the tilt is fitted up to — **not Nyquist**: the resampler's
/// anti-alias filter falls off a cliff near 8 kHz, and a fit through it would
/// report the filter as a property of every speaker. 5 kHz is clear of it and
/// still spans four octaves.
pub const TILT_HIGH_HZ: f32 = 5000.0;

/// Floor added to every bin before the flatness ratio, so digital silence is not
/// "perfectly tonal" (a geometric mean of zeros).
const BIN_FLOOR: f32 = 1e-10;

/// Per-frame description of the noise, measured above [`NOISE_BAND_LOW_HZ`].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Texture {
    /// Spectral centroid per frame, in Hz — where the energy sits.
    pub centroid_hz: Vec<f32>,
    /// Spectral flatness per frame, 0..1: near zero for a vowel, high for a
    /// fricative.
    pub flatness: Vec<f32>,
    /// Spectral tilt per frame, in dB per octave (negative falls away), fitted
    /// between [`NOISE_BAND_LOW_HZ`] and [`TILT_HIGH_HZ`].
    pub tilt_db_per_octave: Vec<f32>,
}

/// Measure the centroid, flatness and tilt of every frame, from its spectrum
/// ([`crate::frame::spectra`]).
pub fn track(spectra: &[Vec<Complex32>]) -> Texture {
    let n = spectra.len();
    if n == 0 {
        return Texture {
            centroid_hz: Vec::new(),
            flatness: Vec::new(),
            tilt_db_per_octave: Vec::new(),
        };
    }

    let bins = SPECTRAL_WINDOW / 2 + 1;
    let bin_hz = ANALYSIS_RATE as f32 / SPECTRAL_WINDOW as f32;
    let lowest = ((NOISE_BAND_LOW_HZ / bin_hz).ceil() as usize).min(bins - 1);
    let highest = ((TILT_HIGH_HZ / bin_hz).floor() as usize).min(bins - 1);

    // The fit's abscissa is the same every frame, so it is computed once.
    let octaves: Vec<f32> = (lowest..=highest)
        .map(|k| (k as f32 * bin_hz / NOISE_BAND_LOW_HZ).log2())
        .collect();
    let octave_mean = octaves.iter().sum::<f32>() / octaves.len().max(1) as f32;
    let octave_spread: f32 = octaves.iter().map(|o| (o - octave_mean).powi(2)).sum();

    let mut centroid_hz = vec![0.0f32; n];
    let mut flatness = vec![0.0f32; n];
    let mut tilt_db_per_octave = vec![0.0f32; n];

    for (i, spectrum) in spectra.iter().enumerate() {
        // Power, not magnitude: flatness is defined on the power spectrum.
        let power: Vec<f32> = spectrum[lowest..bins]
            .iter()
            .map(|c| c.norm_sqr() + BIN_FLOOR)
            .collect();

        let total: f32 = power.iter().sum();
        centroid_hz[i] = if total <= 0.0 {
            0.0
        } else {
            power
                .iter()
                .enumerate()
                .map(|(k, p)| (k + lowest) as f32 * bin_hz * p)
                .sum::<f32>()
                / total
        };

        // Geometric mean in the log domain; the direct product underflows.
        let log_mean = power.iter().map(|p| p.ln()).sum::<f32>() / power.len() as f32;
        let arithmetic_mean = total / power.len() as f32;
        flatness[i] = if arithmetic_mean <= 0.0 {
            0.0
        } else {
            (log_mean.exp() / arithmetic_mean).clamp(0.0, 1.0)
        };

        // Least squares in dB against octaves: against linear frequency the top
        // octave, half the bins, would decide the answer.
        if octave_spread > 0.0 {
            let decibels: Vec<f32> = power[..=highest - lowest]
                .iter()
                .map(|p| 10.0 * p.log10())
                .collect();
            let db_mean = decibels.iter().sum::<f32>() / decibels.len() as f32;
            let covariance: f32 = octaves
                .iter()
                .zip(&decibels)
                .map(|(o, d)| (o - octave_mean) * (d - db_mean))
                .sum();
            tilt_db_per_octave[i] = covariance / octave_spread;
        }
    }

    Texture {
        centroid_hz,
        flatness,
        tilt_db_per_octave,
    }
}
