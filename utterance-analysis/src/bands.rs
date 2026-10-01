//! Periodicity per frequency band.
//!
//! YIN's aperiodicity is one number for the whole spectrum, and the low
//! harmonics carry most of the energy, so it mostly describes them. A breathy
//! voice is periodic low and noisy high — turbulence at a glottis that does not
//! fully close — which a full-band figure barely registers. This measures each
//! band on its own terms.
//!
//! A probe, not yet part of the voiceprint: `cargo run --bin streams` decides
//! whether it says anything the existing streams do not.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;

use crate::frame::{self, PITCH_WINDOW};
use crate::resample::ANALYSIS_RATE;

/// Where the fundamental and the first harmonics live: the band YIN's figure is
/// already dominated by.
pub const LOW_HZ: (f32, f32) = (80.0, 1000.0);

/// Where breath noise competes with the harmonics. Above the first two
/// formants, below the analysis Nyquist.
pub const HIGH_HZ: (f32, f32) = (2000.0, 6000.0);

/// Width of the cosine skirt at each band edge. A brick wall rings for the
/// whole take after every onset.
const SKIRT_HZ: f32 = 100.0;

/// Aperiodicity per frame in each band, 0 (periodic) to 1, on voiced frames
/// only — a period is needed to measure against.
#[derive(Clone, Debug, PartialEq)]
pub struct BandAperiodicity {
    pub low: Vec<Option<f32>>,
    pub high: Vec<Option<f32>>,
}

/// Measure both bands across `samples` (mono, at [`ANALYSIS_RATE`]) at the
/// periods `pitch_hz` gives.
///
/// The low band is read as a waveform. The high band is read as its envelope:
/// jitter of a fraction of a percent shifts a 3 kHz harmonic by a sizeable part
/// of its cycle from one period to the next, but every glottal closure still
/// excites the high band at once, so a periodic voice pulses there at f0 while
/// breath noise does not.
pub fn track(samples: &[f32], pitch_hz: &[Option<f32>]) -> BandAperiodicity {
    BandAperiodicity {
        low: waveform(samples, pitch_hz, LOW_HZ),
        high: envelope(samples, pitch_hz, HIGH_HZ),
    }
}

/// Aperiodicity of the waveform in `band`, per frame.
fn waveform(samples: &[f32], pitch_hz: &[Option<f32>], band: (f32, f32)) -> Vec<Option<f32>> {
    let x: Vec<f32> = analytic(samples, band).iter().map(|z| z.re).collect();
    per_frame(&x, pitch_hz, |w, _| w.to_vec())
}

/// Aperiodicity of the envelope in `band`, per frame.
fn envelope(samples: &[f32], pitch_hz: &[Option<f32>], band: (f32, f32)) -> Vec<Option<f32>> {
    let x: Vec<f32> = analytic(samples, band).iter().map(|z| z.norm()).collect();
    per_frame(&x, pitch_hz, detrend)
}

/// `prepare` each voiced frame's window of `x` and measure it at its period.
fn per_frame(
    x: &[f32],
    pitch_hz: &[Option<f32>],
    prepare: impl Fn(&[f32], f32) -> Vec<f32>,
) -> Vec<Option<f32>> {
    frame::frames(x.len())
        .zip(pitch_hz)
        .map(|(f, hz)| {
            let period = ANALYSIS_RATE as f32 / (*hz)?;
            aperiodicity(
                &prepare(&frame::windowed(x, f, PITCH_WINDOW), period),
                period,
            )
        })
        .collect()
}

/// The analytic signal of `samples` restricted to `band`: its real part is the
/// band-limited waveform, its magnitude the band's envelope. Done over the whole
/// take at once, so no frame window shapes the envelope.
fn analytic(samples: &[f32], band: (f32, f32)) -> Vec<Complex32> {
    let n = samples.len().max(1).next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let mut buf: Vec<Complex32> = samples
        .iter()
        .map(|&s| Complex32::new(s, 0.0))
        .chain(std::iter::repeat(Complex32::new(0.0, 0.0)))
        .take(n)
        .collect();
    planner.plan_fft_forward(n).process(&mut buf);

    let (lo, hi) = band;
    for (k, bin) in buf.iter_mut().enumerate() {
        let hz = k as f32 * ANALYSIS_RATE as f32 / n as f32;
        // Negative frequencies dropped and positive ones doubled: the analytic
        // signal. The 1/n of the inverse transform is folded in here.
        let gain = if k == 0 || k >= n / 2 {
            0.0
        } else {
            2.0 * skirt(hz - lo) * skirt(hi - hz) / n as f32
        };
        *bin *= gain;
    }
    planner.plan_fft_inverse(n).process(&mut buf);
    buf.truncate(samples.len());
    buf
}

/// 0 below the edge, 1 a skirt's width inside it, a raised cosine between.
fn skirt(inside_hz: f32) -> f32 {
    if inside_hz <= 0.0 {
        0.0
    } else if inside_hz >= SKIRT_HZ {
        1.0
    } else {
        0.5 * (1.0 - (std::f32::consts::PI * inside_hz / SKIRT_HZ).cos())
    }
}

/// `x` less its moving average over one period. An average over exactly one
/// period cancels f0 and every harmonic, so what it removes is the slow part —
/// a level ramp, which would correlate with itself at any lag and read as
/// periodic.
fn detrend(x: &[f32], period: f32) -> Vec<f32> {
    let half = (period / 2.0).round() as usize;
    let mut prefix = vec![0.0f32; x.len() + 1];
    for (i, v) in x.iter().enumerate() {
        prefix[i + 1] = prefix[i] + v;
    }
    (0..x.len())
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(x.len());
            x[i] - (prefix[hi] - prefix[lo]) / (hi - lo) as f32
        })
        .collect()
}

/// One minus the normalised correlation of `x` with itself one period later,
/// best of the lags either side of the period so a fractional period is not
/// penalised. `None` where the window is silent.
fn aperiodicity(x: &[f32], period: f32) -> Option<f32> {
    let centre = period.round() as usize;
    let best = (centre.saturating_sub(1)..=centre + 1)
        .filter(|&lag| lag > 0 && lag < x.len())
        .filter_map(|lag| {
            let (a, b) = (&x[..x.len() - lag], &x[lag..]);
            let ab: f32 = a.iter().zip(b).map(|(p, q)| p * q).sum();
            let aa: f32 = a.iter().map(|p| p * p).sum();
            let bb: f32 = b.iter().map(|q| q * q).sum();
            (aa > f32::EPSILON && bb > f32::EPSILON).then(|| ab / (aa * bb).sqrt())
        })
        .reduce(f32::max)?;
    Some((1.0 - best).clamp(0.0, 1.0))
}
