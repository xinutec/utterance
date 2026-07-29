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
//! - **tilt** — how fast the spectrum falls away with frequency, in dB per
//!   octave. The correlate of vocal effort: a pressed or shouted voice has a
//!   shallow tilt because the glottis closes abruptly and throws energy high, a
//!   breathy or relaxed one falls off steeply. Centroid says *where* the energy
//!   sits; tilt says *how it is distributed*, and a voice can move either
//!   without moving the other.
//!
//! All three are computed for every frame, voiced or not. They are defined
//! everywhere, and deciding where they are *interesting* is the mapping layer's
//! business rather than this one's.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::frame::{self, SPECTRAL_WINDOW};
use crate::resample::ANALYSIS_RATE;

/// Lowest frequency either measure looks at.
///
/// Both are measured above this rather than across everything, and the reason is
/// empirical: on real speech the unvoiced frames came back with a median
/// centroid of 153 Hz and a flatness of 0.001 — reading as *tonal* — because a
/// room's rumble, a microphone's proximity boost and the tail of the previous
/// vowel all pile up at the bottom of the spectrum and dominate the average. A
/// fricative's energy lives from about 2 kHz up, so a measure swamped by the
/// bottom octave describes the room instead of the consonant.
///
/// Set below the lowest fricative energy and above where rumble lives. It makes
/// these measurements *about* the band consonants occupy, which is the point of
/// having them.
pub const NOISE_BAND_LOW_HZ: f32 = 300.0;

/// Highest frequency the tilt is fitted up to.
///
/// **Not Nyquist, and this is the whole correctness of the measurement.**
/// Everything is resampled to 16 kHz, and a band-limited resampler's
/// anti-aliasing filter falls off a cliff approaching 8 kHz. Fitting a slope
/// through that would measure the filter — steeply, consistently, and on every
/// frame of every recording — and report it as a property of the speaker. Fitted
/// to 5 kHz instead, which is clear of the transition band and still spans four
/// octaves of the band a voice actually radiates into.
///
/// The same reasoning as [`NOISE_BAND_LOW_HZ`] at the other end, and the same
/// failure it was written for: a measure whose average is dominated by something
/// that is not the voice.
const TILT_HIGH_HZ: f32 = 5000.0;

/// Floor added to every bin before the flatness ratio.
///
/// A geometric mean collapses to zero if any single bin is zero, which in a
/// digital silence is all of them — so without this, flatness reports "perfectly
/// tonal" for a frame containing nothing at all. Small enough to be far below
/// any real signal.
const BIN_FLOOR: f32 = 1e-10;

/// Per-frame description of the noise in a recording.
///
/// Both series are measured above [`NOISE_BAND_LOW_HZ`], so they describe the
/// band consonants occupy rather than the whole spectrum.
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
    /// Spectral tilt per frame, in dB per octave. Negative falls away.
    ///
    /// Fitted between [`NOISE_BAND_LOW_HZ`] and [`TILT_HIGH_HZ`], so it describes
    /// the voice rather than the room below it or the resampler above it.
    pub tilt_db_per_octave: Vec<f32>,
}

/// Measure the centroid and flatness of every frame.
pub fn track(samples: &[f32]) -> Texture {
    let n = frame::count(samples.len());
    if n == 0 {
        return Texture {
            centroid_hz: Vec::new(),
            flatness: Vec::new(),
            tilt_db_per_octave: Vec::new(),
        };
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(SPECTRAL_WINDOW);
    let window = frame::hann(SPECTRAL_WINDOW);
    let bins = SPECTRAL_WINDOW / 2 + 1; // real input: the upper half mirrors.
    let bin_hz = ANALYSIS_RATE as f32 / SPECTRAL_WINDOW as f32;
    let lowest = ((NOISE_BAND_LOW_HZ / bin_hz).ceil() as usize).min(bins - 1);
    let highest = ((TILT_HIGH_HZ / bin_hz).floor() as usize).min(bins - 1);

    // The abscissa of the tilt fit never changes from frame to frame, so the
    // octave positions and their spread are computed once. What varies is only
    // the power in each bin.
    let octaves: Vec<f32> = (lowest..=highest)
        .map(|k| (k as f32 * bin_hz / NOISE_BAND_LOW_HZ).log2())
        .collect();
    let octave_mean = octaves.iter().sum::<f32>() / octaves.len().max(1) as f32;
    let octave_spread: f32 = octaves.iter().map(|o| (o - octave_mean).powi(2)).sum();

    let mut centroid_hz = vec![0.0f32; n];
    let mut flatness = vec![0.0f32; n];
    let mut tilt_db_per_octave = vec![0.0f32; n];
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
        let power: Vec<f32> = buf[lowest..bins]
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

        // Least squares in dB against octaves, which is what "dB per octave"
        // means and is also the domain the ear works in: a fit against linear
        // frequency would let the top octave, holding half the bins, decide the
        // answer on its own.
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
