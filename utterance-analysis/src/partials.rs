//! The measured harmonic series of a voice: for each harmonic of f0, where it
//! sat and how strong it was.
//!
//! **The amplitudes are the payload.** A voice is very nearly harmonic, so the
//! ratios should be close to whole numbers; what differs between people is which
//! partials their tract emphasises, and that is what a tuning is derived from.
//! It belongs to a vowel as much as a speaker, which is why calibration asks for
//! a *steady* one. Only voiced frames near the take's median pitch are used, and
//! [`Partials::frames_used`] says how many there were.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::frame;
use crate::resample::ANALYSIS_RATE;

/// Highest harmonic looked for: near 3 kHz for a low voice, past where partials
/// matter to beating.
pub const MAX_PARTIAL: usize = 24;

/// Window for the harmonic measurement, 128 ms: bins 7.8 Hz apart, so even a low
/// voice's harmonics are resolved. A steady vowel makes the long window free.
pub const PARTIAL_WINDOW: usize = 2048;

/// How far a frame's pitch may sit from the take's median and still be used, in
/// semitones: a frame on another note smears every partial, the top worst.
const PITCH_TOLERANCE_SEMITONES: f32 = 1.0;

/// Fraction of f0, either side of a harmonic's predicted position, searched for
/// its peak. Wider would let a slightly wrong f0 lock harmonic *k* onto its
/// neighbour — a clean and wrong series.
const SEARCH_FRACTION: f32 = 0.33;

/// Level below the frame's strongest partial at which a peak stops counting, in
/// dB: 60 dB down is the noise floor, not the voice.
const FLOOR_DB: f32 = -60.0;

/// Fraction of usable frames a harmonic must appear in to be reported. Rarer
/// ones would sit beside real measurements as if equally solid; the rest carry
/// [`Partial::presence`] so a consumer can weight them.
const MIN_PRESENCE: f32 = 0.5;

/// One harmonic of the measured series.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Partial {
    /// Which harmonic this is. 1 is the fundamental.
    pub number: u32,
    /// Measured frequency over measured f0, median across frames — agreement
    /// between two estimates, close to `number`.
    pub ratio: f32,
    /// Median amplitude, relative to the strongest partial in the take.
    pub amplitude: f32,
    /// Fraction of usable frames this harmonic was found in.
    pub presence: f32,
}

/// The harmonic series of one recording.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Partials {
    /// Frames voiced and near enough the median pitch to use: hundreds for a
    /// sustained vowel, few for speech.
    pub frames_used: usize,
    /// Median f0 across those frames, the reference every ratio is against.
    pub f0_hz: Option<f32>,
    /// Harmonics found often enough to report, ascending by number.
    pub partials: Vec<Partial>,
}

/// Measure the harmonic series of `samples`, from the voiceprint's own pitch
/// track so every ratio is against the same fundamental.
pub fn measure(samples: &[f32], pitch: &[Option<f32>]) -> Partials {
    let Some(median_f0) = median(&pitch.iter().flatten().copied().collect::<Vec<_>>()) else {
        return Partials {
            frames_used: 0,
            f0_hz: None,
            partials: Vec::new(),
        };
    };

    let tolerance = 2f32.powf(PITCH_TOLERANCE_SEMITONES / 12.0);
    let usable: Vec<(usize, f32)> = pitch
        .iter()
        .enumerate()
        .filter_map(|(i, hz)| Some((i, (*hz)?)))
        .filter(|(_, hz)| *hz < median_f0 * tolerance && *hz > median_f0 / tolerance)
        .collect();

    if usable.is_empty() {
        return Partials {
            frames_used: 0,
            f0_hz: Some(median_f0),
            partials: Vec::new(),
        };
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(PARTIAL_WINDOW);
    let window = frame::blackman(PARTIAL_WINDOW);
    let bin_hz = ANALYSIS_RATE as f32 / PARTIAL_WINDOW as f32;

    // Per harmonic, every frame's observation of it.
    let mut ratios: Vec<Vec<f32>> = vec![Vec::new(); MAX_PARTIAL + 1];
    let mut amplitudes: Vec<Vec<f32>> = vec![Vec::new(); MAX_PARTIAL + 1];

    let mut buf = vec![Complex32::new(0.0, 0.0); PARTIAL_WINDOW];
    for &(index, f0) in &usable {
        let block = frame::windowed(samples, index, PARTIAL_WINDOW);
        for (b, (s, w)) in buf.iter_mut().zip(block.iter().zip(&window)) {
            *b = Complex32::new(s * w, 0.0);
        }
        fft.process(&mut buf);

        // Real input: the upper half mirrors the lower.
        let magnitude: Vec<f32> = buf[..PARTIAL_WINDOW / 2].iter().map(|c| c.norm()).collect();

        let observed = harmonics(&magnitude, f0, bin_hz);
        let Some(loudest) = observed
            .iter()
            .flatten()
            .map(|(_, a)| *a)
            .max_by(f32::total_cmp)
        else {
            continue;
        };
        let floor = loudest * 10f32.powf(FLOOR_DB / 20.0);

        for (k, found) in observed.iter().enumerate() {
            if let Some((hz, amplitude)) = *found
                && amplitude >= floor
            {
                ratios[k].push(hz / f0);
                amplitudes[k].push(amplitude);
            }
        }
    }

    // Normalised to the loudest harmonic overall, so the profile is one
    // spectrum's shape rather than an average of per-frame shapes.
    let peak = amplitudes
        .iter()
        .filter_map(|a| median(a))
        .max_by(f32::total_cmp)
        .unwrap_or(1.0);

    let partials = (1..=MAX_PARTIAL)
        .filter_map(|k| {
            let presence = amplitudes[k].len() as f32 / usable.len() as f32;
            if presence < MIN_PRESENCE {
                return None;
            }
            Some(Partial {
                number: k as u32,
                ratio: median(&ratios[k])?,
                amplitude: median(&amplitudes[k])? / peak,
                presence,
            })
        })
        .collect();

    Partials {
        frames_used: usable.len(),
        f0_hz: Some(median_f0),
        partials,
    }
}

/// Peak frequency and amplitude for each harmonic of `f0`, indexed by harmonic
/// number. Index 0 is always `None` so `k` indexes harmonic `k`.
fn harmonics(magnitude: &[f32], f0: f32, bin_hz: f32) -> Vec<Option<(f32, f32)>> {
    let mut found = vec![None; MAX_PARTIAL + 1];
    let half_band = (f0 * SEARCH_FRACTION / bin_hz).max(1.0);

    for (k, slot) in found.iter_mut().enumerate().skip(1) {
        let centre = k as f32 * f0 / bin_hz;
        let lo = (centre - half_band).floor().max(1.0) as usize;
        let hi = ((centre + half_band).ceil() as usize).min(magnitude.len() - 2);
        if lo > hi {
            break;
        }

        let peak = (lo..=hi).fold(lo, |best, i| {
            if magnitude[i] > magnitude[best] {
                i
            } else {
                best
            }
        });
        // A peak pinned to its band's edge is the shoulder of something else.
        if peak == lo || peak == hi {
            continue;
        }
        *slot = Some(interpolate(magnitude, peak, bin_hz));
    }
    found
}

/// Refine a magnitude peak with a parabola through it and its neighbours —
/// otherwise a ratio is quantised to the bin spacing, several percent at f0.
fn interpolate(magnitude: &[f32], peak: usize, bin_hz: f32) -> (f32, f32) {
    let (a, b, c) = (magnitude[peak - 1], magnitude[peak], magnitude[peak + 1]);
    let denominator = a - 2.0 * b + c;
    // Flat or symmetric: the bin centre is the answer.
    let offset = if denominator.abs() < f32::EPSILON {
        0.0
    } else {
        0.5 * (a - c) / denominator
    };
    let amplitude = b - 0.25 * (a - c) * offset;
    ((peak as f32 + offset) * bin_hz, amplitude)
}

/// Median of an unsorted slice, or `None` when there is nothing to take.
fn median(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    Some(sorted[sorted.len() / 2])
}
