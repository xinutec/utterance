//! Signal generators shared by the analysis test binaries.
//!
//! Synthetic rather than recorded on purpose: the point of these tests is that
//! the analyser reports the *right* answer, and only a generated signal has a
//! known-correct one. Real speech has no ground-truth f0 to compare against. A
//! recorded fixture belongs in the tests that judge whether the analyser is
//! useful, which is a separate question from whether it is correct.

// Each test binary compiles this module in full but uses only the generators it
// needs, so anything another binary uses reads as dead code here.
#![allow(dead_code)]

use music_analysis::resample::ANALYSIS_RATE;

/// A pure sine at `freq`, in seconds of audio at `rate`.
pub fn sine(freq: f64, rate: u32, secs: f64) -> Vec<f32> {
    let n = (f64::from(rate) * secs) as usize;
    (0..n)
        .map(|i| {
            let t = (i as f64) / f64::from(rate);
            (2.0 * std::f64::consts::PI * freq * t).sin() as f32
        })
        .collect()
}

/// A sawtooth at [`ANALYSIS_RATE`].
///
/// Not a sine: a single partial makes pitch tracking easier than anything real.
/// A sawtooth carries the full harmonic series like a glottal source, so it
/// exercises the octave logic that a sine never reaches.
pub fn saw(freq: f32, secs: f32) -> Vec<f32> {
    let n = (ANALYSIS_RATE as f32 * secs) as usize;
    (0..n)
        .map(|i| {
            let phase = (freq * i as f32 / ANALYSIS_RATE as f32).fract();
            2.0 * phase - 1.0
        })
        .collect()
}

/// A crude two-formant vowel, synthesised source-filter style: a harmonic series
/// at `f0_hz` scaled by a spectral envelope with peaks at 700 Hz and 1220 Hz.
///
/// The formants shape harmonics that are already there — they are not added as
/// separate tones. Adding a 700 Hz sinusoid to a 120 Hz source would make the
/// signal inharmonic and genuinely unpitched, and a pitch tracker would be right
/// to refuse it.
pub fn vowel(f0_hz: f32, secs: f32) -> Vec<f32> {
    let n = (ANALYSIS_RATE as f32 * secs) as usize;
    let harmonics = harmonic_series(f0_hz, ANALYSIS_RATE as f32 / 2.0);

    (0..n)
        .map(|i| {
            let t = i as f32 / ANALYSIS_RATE as f32;
            harmonics
                .iter()
                .map(|&(hz, gain)| gain * (2.0 * std::f32::consts::PI * hz * t).sin())
                .sum::<f32>()
                * 0.5
        })
        .collect()
}

/// Harmonics of `f0_hz` below `ceiling`, with a glottal rolloff and a
/// two-formant vocal-tract envelope.
fn harmonic_series(f0_hz: f32, ceiling: f32) -> Vec<(f32, f32)> {
    fn formant(hz: f32, center: f32, bandwidth: f32) -> f32 {
        1.0 / (1.0 + ((hz - center) / bandwidth).powi(2))
    }
    (1..)
        .map(|k| k as f32 * f0_hz)
        .take_while(|&hz| hz < ceiling)
        // -6 dB/octave glottal rolloff, then the vocal-tract envelope.
        .map(|hz| {
            (
                hz,
                (f0_hz / hz) * (formant(hz, 700.0, 100.0) + 0.5 * formant(hz, 1220.0, 120.0)),
            )
        })
        .collect()
}

/// 100 ms tone bursts starting at each of `times_s`, in `total_s` of silence.
pub fn bursts(times_s: &[f32], total_s: f32) -> Vec<f32> {
    let n = (ANALYSIS_RATE as f32 * total_s) as usize;
    let mut x = vec![0.0f32; n];
    for &t in times_s {
        let start = (t * ANALYSIS_RATE as f32) as usize;
        for k in 0..ANALYSIS_RATE as usize / 10 {
            if start + k >= n {
                break;
            }
            let phase = 2.0 * std::f32::consts::PI * 440.0 * (k as f32) / ANALYSIS_RATE as f32;
            x[start + k] = phase.sin() * 0.8;
        }
    }
    x
}

/// Deterministic white noise from a linear congruential generator.
///
/// Its own generator rather than the `rand` crate: the sequence must be
/// identical on every run and every machine, so a failure is reproducible.
pub fn noise(secs: f32) -> Vec<f32> {
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    (0..(ANALYSIS_RATE as f32 * secs) as usize)
        .map(|_| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((seed >> 33) as f32 / (1u64 << 31) as f32) - 1.0
        })
        .collect()
}

/// Encode samples as a 16-bit mono WAV at `rate`.
pub fn wav_bytes(samples: &[f32], rate: u32) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut buf, spec).expect("wav writer");
        for &s in samples {
            w.write_sample((s.clamp(-1.0, 1.0) * 32_767.0) as i16)
                .expect("write sample");
        }
        w.finalize().expect("finalize wav");
    }
    buf.into_inner()
}

/// Count of positive-going zero crossings, i.e. cycles present in the signal.
pub fn cycles(x: &[f32]) -> usize {
    x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count()
}

/// Root-mean-square level of a signal.
pub fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / (x.len() as f32)).sqrt()
}

/// Synthesise a vowel through an actual resonator cascade.
///
/// A glottal impulse train driven through one two-pole resonator per formant —
/// the source-filter model itself, rather than an approximation of its output.
/// That matters for testing formant tracking: linear prediction fits exactly
/// this structure, so a signal built this way has formants at frequencies that
/// are *known* rather than merely intended, and the tracker either recovers them
/// or is wrong.
pub fn resonated_vowel(f0_hz: f32, formants: &[(f32, f32)], secs: f32) -> Vec<f32> {
    let n = (ANALYSIS_RATE as f32 * secs) as usize;
    let period = (ANALYSIS_RATE as f32 / f0_hz).round() as usize;

    // Glottal source: an impulse train. Flat spectrum, so every resonance in the
    // filter below shows up in the output with nothing else shaping it.
    let mut signal: Vec<f32> = (0..n)
        .map(|i| if i % period == 0 { 1.0 } else { 0.0 })
        .collect();

    for &(frequency, bandwidth) in formants {
        let theta = 2.0 * std::f32::consts::PI * frequency / ANALYSIS_RATE as f32;
        let radius = (-std::f32::consts::PI * bandwidth / ANALYSIS_RATE as f32).exp();
        let (a1, a2) = (2.0 * radius * theta.cos(), -radius * radius);

        let mut y1 = 0.0f32;
        let mut y2 = 0.0f32;
        for sample in &mut signal {
            let y = *sample + a1 * y1 + a2 * y2;
            y2 = y1;
            y1 = y;
            *sample = y;
        }
    }

    // Normalise to a sane level; the resonator cascade has large gain.
    let peak = signal.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        for s in &mut signal {
            *s /= peak * 1.2;
        }
    }
    signal
}
