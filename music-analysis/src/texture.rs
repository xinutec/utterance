//! The shape of the noise in a voice.
//!
//! Nearly three quarters of ordinary speech carries no fundamental: the
//! consonants, and the silences between phrases. Everything else in this crate
//! gates on voicing, so all of that has been measured only as a place where
//! pitch was absent — which throws away the loudest, sharpest and most
//! individual material a speaker produces. Nobody's *s* sounds like anyone
//! else's.
//!
//! **This characterises noise rather than classifying phones.** Knowing a frame
//! is an /s/ is a linguistic label; knowing its energy sits around 7 kHz in a
//! wide band is a measurement, and it is the one a synthesiser can actually act
//! on. Two numbers per frame carry it:
//!
//! - **centroid** — where the energy sits. The standard correlate of
//!   brightness, and what separates a hissed *s* from a hushed *sh* from a
//!   breathy *f*.
//! - **flatness** — how noise-like the spectrum is, from 0 for a pure tone to 1
//!   for white noise. What separates a fricative from a vowel, and a sustained
//!   hiss from a plosive burst.
//!
//! Both are computed for every frame, voiced or not. They are defined
//! everywhere, and deciding where they are *interesting* is the mapping layer's
//! business rather than this one's.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::frame::{self, SPECTRAL_WINDOW};
use crate::resample::ANALYSIS_RATE;

/// Floor added to every bin before the flatness ratio.
///
/// A geometric mean collapses to zero if any single bin is zero, which in a
/// digital silence is all of them — so without this, flatness reports "perfectly
/// tonal" for a frame containing nothing at all. Small enough to be far below
/// any real signal.
const BIN_FLOOR: f32 = 1e-10;

/// Per-frame description of the noise in a recording.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Texture {
    /// Spectral centroid per frame, in Hz — where the energy sits.
    pub centroid_hz: Vec<f32>,
    /// Spectral flatness per frame, 0..1 — how noise-like it is.
    ///
    /// A vowel sits near zero: its energy is concentrated in harmonics. A
    /// fricative sits high: its energy is spread across everything.
    pub flatness: Vec<f32>,
}

/// Measure the centroid and flatness of every frame.
pub fn track(samples: &[f32]) -> Texture {
    let n = frame::count(samples.len());
    if n == 0 {
        return Texture {
            centroid_hz: Vec::new(),
            flatness: Vec::new(),
        };
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(SPECTRAL_WINDOW);
    let window = frame::hann(SPECTRAL_WINDOW);
    let bins = SPECTRAL_WINDOW / 2 + 1; // real input: the upper half mirrors.
    let bin_hz = ANALYSIS_RATE as f32 / SPECTRAL_WINDOW as f32;

    let mut centroid_hz = vec![0.0f32; n];
    let mut flatness = vec![0.0f32; n];
    let mut buf = vec![Complex32::new(0.0, 0.0); SPECTRAL_WINDOW];

    for i in 0..n {
        let frame_samples = frame::windowed(samples, i, SPECTRAL_WINDOW);
        for (b, (s, w)) in buf.iter_mut().zip(frame_samples.iter().zip(&window)) {
            *b = Complex32::new(s * w, 0.0);
        }
        fft.process(&mut buf);

        // Power rather than magnitude: flatness is defined on the power
        // spectrum, and using magnitudes would report every frame flatter than
        // it is.
        let power: Vec<f32> = buf[..bins]
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
                .map(|(k, p)| k as f32 * bin_hz * p)
                .sum::<f32>()
                / total
        };

        // Geometric over arithmetic mean, taken in the log domain: the direct
        // product of five hundred bins underflows to zero long before it means
        // anything.
        let log_mean = power.iter().map(|p| p.ln()).sum::<f32>() / power.len() as f32;
        let arithmetic_mean = total / power.len() as f32;
        flatness[i] = if arithmetic_mean <= 0.0 {
            0.0
        } else {
            (log_mean.exp() / arithmetic_mean).clamp(0.0, 1.0)
        };
    }

    Texture {
        centroid_hz,
        flatness,
    }
}
