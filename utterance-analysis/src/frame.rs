//! The analysis frame grid: every per-frame series shares one frame number.
//! Analyses vary their *window*, never their hop.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::resample::ANALYSIS_RATE;

/// Samples between consecutive frames — 10 ms, the speech-analysis convention.
pub const HOP: usize = ANALYSIS_RATE as usize / 100;

/// Window for pitch estimation, 64 ms: YIN's lag search reaches one period at
/// 70 Hz (229 samples) and needs a window twice the longest lag.
pub const PITCH_WINDOW: usize = 1024;

/// Window for spectral analysis, 32 ms. Shorter than the pitch window because
/// onset detection wants time resolution, not frequency resolution.
pub const SPECTRAL_WINDOW: usize = 512;

/// A position on the frame grid, and an index into every per-frame series.
///
/// Its own type because a frame number, a sample offset and a window length are
/// all `usize`, and one passed as another still compiles. A newtype, so JSON
/// carries the bare number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Frame(pub usize);

impl Frame {
    /// The sample this frame is centred on.
    pub fn sample(self) -> usize {
        self.0 * HOP
    }

    /// Start time in seconds on a grid `hop_s` apart.
    pub fn time_s(self, hop_s: f32) -> f32 {
        self.0 as f32 * hop_s
    }
}

/// Number of frames covering `len` samples.
pub fn count(len: usize) -> usize {
    if len == 0 { 0 } else { len.div_ceil(HOP) }
}

/// Every frame covering `len` samples, in order.
pub fn frames(len: usize) -> impl Iterator<Item = Frame> {
    (0..count(len)).map(Frame)
}

/// Copy the `window`-sample window centred on `frame`, zero-padded at the
/// edges. Centred, so measurements describe the audio *at* the timestamp.
pub fn windowed(samples: &[f32], frame: Frame, window: usize) -> Vec<f32> {
    let center = frame.sample() as isize;
    let start = center - (window as isize) / 2;
    (0..window)
        .map(|k| {
            let idx = start + k as isize;
            if idx < 0 || idx as usize >= samples.len() {
                0.0
            } else {
                samples[idx as usize]
            }
        })
        .collect()
}

/// The Hann-windowed [`SPECTRAL_WINDOW`] spectrum of every frame, non-negative
/// bins only — computed once and shared.
pub fn spectra(samples: &[f32]) -> Vec<Vec<Complex32>> {
    let fft = FftPlanner::<f32>::new().plan_fft_forward(SPECTRAL_WINDOW);
    let window = hann(SPECTRAL_WINDOW);
    let bins = SPECTRAL_WINDOW / 2 + 1;
    let mut buf = vec![Complex32::new(0.0, 0.0); SPECTRAL_WINDOW];
    frames(samples.len())
        .map(|frame| {
            for (b, (s, w)) in buf.iter_mut().zip(
                windowed(samples, frame, SPECTRAL_WINDOW)
                    .iter()
                    .zip(&window),
            ) {
                *b = Complex32::new(s * w, 0.0);
            }
            fft.process(&mut buf);
            buf[..bins].to_vec()
        })
        .collect()
}

/// Periodic Hann window of length `n` — the form that sums to a constant under
/// overlap.
pub fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
            0.5 * (1.0 - x.cos())
        })
        .collect()
}

/// Hamming window of length `n`, for linear prediction: its low first sidelobe
/// keeps one harmonic out of its neighbours' autocorrelation.
pub fn hamming(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
            0.54 - 0.46 * x.cos()
        })
        .collect()
}

/// Blackman window of length `n`, for partials: fast-falling sidelobes, so a
/// strong harmonic does not inflate its neighbour.
pub fn blackman(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
            0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos()
        })
        .collect()
}
