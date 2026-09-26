//! Formant tracking: where the vocal tract resonates, frame by frame.
//!
//! Formants make one vowel differ from another and barely move with pitch, so
//! they describe what the mouth is doing apart from the note. F1 against F2 is
//! the space vowels occupy, and what the harmony mappings are built on.

use crate::frame::{self, SPECTRAL_WINDOW};
use crate::lpc;
use crate::resample::ANALYSIS_RATE;

/// Lowest frequency accepted as a formant. Below any F1, and above the region
/// where the residual spectral tilt puts spurious poles.
const F_MIN_HZ: f32 = 90.0;

/// Highest frequency accepted as a formant. Above any F3, comfortably below the
/// 8 kHz Nyquist where poles are unreliable.
const F_MAX_HZ: f32 = 5_000.0;

/// Widest pole accepted as a formant: a resonance is tens to a couple of hundred
/// hertz wide, and a wider pole is the fit describing the spectrum's overall
/// shape — how spurious formants appear in silence and fricatives.
const BANDWIDTH_MAX_HZ: f32 = 400.0;

/// The first three formants of one frame, `None` where there is no usable
/// estimate — never a sentinel 0 Hz to average into a mapping.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FormantFrame {
    pub f1: Option<f32>,
    pub f2: Option<f32>,
    pub f3: Option<f32>,
}

/// A single resonance recovered from the fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resonance {
    pub frequency_hz: f32,
    pub bandwidth_hz: f32,
}

/// Track formants across every frame. `voiced` gates the estimate: without a
/// periodic source the poles describe noise, and would invent vowels.
pub fn track(samples: &[f32], voiced: &[bool]) -> Vec<FormantFrame> {
    (0..frame::count(samples.len()))
        .map(|i| {
            if !voiced.get(i).copied().unwrap_or(false) {
                return FormantFrame::default();
            }
            let window = frame::windowed(samples, i, SPECTRAL_WINDOW);
            estimate(&window)
        })
        .collect()
}

/// Plausible range for each formant, in Hz, across adult speakers — anatomy
/// limits how far jaw and tongue move them.
const RANGES: [(f32, f32); 3] = [(200.0, 1_100.0), (600.0, 3_000.0), (1_500.0, 4_000.0)];

/// Formants of a single windowed frame.
pub fn estimate(window: &[f32]) -> FormantFrame {
    assign(&resonances(window))
}

/// Fit resonances to formant slots, lowest first, within each slot's range.
///
/// Taking the three lowest in order fails when one is missed: everything above
/// shifts down a slot, and on a real glide that put F2 at 3.4 kHz in a fifth of
/// frames. Out-of-range becomes `None` — a mapping can skip a gap, but cannot
/// detect a plausible-looking lie.
fn assign(resonances: &[Resonance]) -> FormantFrame {
    let mut slots: [Option<f32>; 3] = [None; 3];
    let mut next = 0;

    for resonance in resonances {
        // Skip slots this resonance is too high for, so a missing F1 does not
        // take the F2 slot.
        while next < RANGES.len() && resonance.frequency_hz > RANGES[next].1 {
            next += 1;
        }
        if next >= RANGES.len() {
            break;
        }
        if resonance.frequency_hz >= RANGES[next].0 {
            slots[next] = Some(resonance.frequency_hz);
            next += 1;
        }
    }
    FormantFrame {
        f1: slots[0],
        f2: slots[1],
        f3: slots[2],
    }
}

/// Every resonance in the frame, in increasing frequency order.
pub fn resonances(window: &[f32]) -> Vec<Resonance> {
    let emphasised = lpc::pre_emphasise(window);
    let windowed: Vec<f32> = emphasised
        .iter()
        .zip(frame::hamming(emphasised.len()))
        .map(|(s, w)| s * w)
        .collect();

    let Some(coefficients) = lpc::coefficients(&windowed, lpc::ORDER) else {
        return Vec::new();
    };

    let rate = f64::from(ANALYSIS_RATE);
    let mut found: Vec<Resonance> = lpc::roots(&coefficients)
        .into_iter()
        // One of each conjugate pair; a real pole is spectral slope.
        .filter(|z| z.im > 0.0)
        // Outside the unit circle: an unstable fit, describing nothing.
        .filter(|z| z.norm() < 1.0)
        .map(|z| Resonance {
            frequency_hz: (z.arg() * rate / (2.0 * std::f64::consts::PI)) as f32,
            bandwidth_hz: (-z.norm().ln() * rate / std::f64::consts::PI) as f32,
        })
        .filter(|r| (F_MIN_HZ..=F_MAX_HZ).contains(&r.frequency_hz))
        .filter(|r| r.bandwidth_hz <= BANDWIDTH_MAX_HZ)
        .collect();

    found.sort_by(|a, b| a.frequency_hz.total_cmp(&b.frequency_hz));
    found
}
