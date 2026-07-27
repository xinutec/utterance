//! Summing sinusoids.

use music_mapping::score::{Event, Score};

/// Rate everything is rendered at.
///
/// 44.1 kHz because the output is for listening rather than for analysis, and
/// this is what every browser and audio player expects without resampling.
pub const RENDER_RATE: u32 = 44_100;

/// Attack and release of a note, in seconds.
///
/// Short, but never zero: a sinusoid switched on mid-cycle is a step
/// discontinuity, and a step is a click. Long enough to remove that, short
/// enough that the onset still reads as an onset.
const ATTACK_S: f32 = 0.012;
const RELEASE_S: f32 = 0.09;

/// Peak level the finished render is scaled to.
///
/// Under full scale on purpose: notes overlap, and a render that normalises to
/// exactly 1.0 leaves no room for the intersample peaks that appear when it is
/// converted for playback.
const HEADROOM: f32 = 0.89;

/// Render a score to mono samples at [`RENDER_RATE`].
///
/// Deterministic, as everything in this project is: no randomness, no clock, and
/// notes are summed in score order so the floating-point rounding is the same on
/// every run.
pub fn render(score: &Score) -> Vec<f32> {
    let length = (score.duration_s.max(0.0) * RENDER_RATE as f32).ceil() as usize;
    let mut out = vec![0.0f32; length];
    if length == 0 {
        return out;
    }

    // A silent timbre would render silence for every note, which is a confusing
    // way to report "the calibration take had no measurable spectrum". One
    // partial is the honest minimum: a plain sine, obviously unfinished.
    let timbre: &[f32] = if score.timbre.iter().any(|&a| a > 0.0) {
        &score.timbre
    } else {
        &[1.0]
    };

    for event in &score.events {
        sum_note(&mut out, event, timbre);
    }

    normalise(&mut out);
    out
}

/// Add one note to the buffer.
fn sum_note(out: &mut [f32], event: &Event, timbre: &[f32]) {
    let start = (event.start_s * RENDER_RATE as f32).max(0.0) as usize;
    if start >= out.len() || event.hz <= 0.0 {
        return;
    }
    let samples = (event.duration_s * RENDER_RATE as f32) as usize;
    let end = (start + samples).min(out.len());

    // Partials past Nyquist alias down into the audible range as inharmonic
    // rubbish, which would be heard as the tuning being wrong rather than as
    // what it is. Dropping them is the only correct answer.
    let nyquist = RENDER_RATE as f32 / 2.0;
    let voices: Vec<(f32, f32)> = timbre
        .iter()
        .enumerate()
        .map(|(i, &a)| (event.hz * (i + 1) as f32, a))
        .filter(|&(hz, a)| hz < nyquist && a > 0.0)
        .collect();

    // Held at constant power regardless of how many partials survived, so a note
    // low enough to keep all twenty-four is not louder than one that kept six.
    let gain = event.amplitude / voices.iter().map(|(_, a)| a).sum::<f32>().max(f32::EPSILON);

    for (i, sample) in out[start..end].iter_mut().enumerate() {
        let t = i as f32 / RENDER_RATE as f32;
        let envelope = envelope(t, event.duration_s);
        let value: f32 = voices
            .iter()
            .map(|&(hz, a)| a * (std::f32::consts::TAU * hz * t).sin())
            .sum();
        *sample += value * envelope * gain;
    }
}

/// Amplitude envelope: a short fade in, a longer fade out, flat between.
fn envelope(t: f32, duration_s: f32) -> f32 {
    let attack = ATTACK_S.min(duration_s / 2.0);
    let release = RELEASE_S.min(duration_s / 2.0);
    if t < attack {
        t / attack
    } else if t > duration_s - release {
        ((duration_s - t) / release).max(0.0)
    } else {
        1.0
    }
}

/// Scale the whole render so its loudest moment sits at [`HEADROOM`].
///
/// Whole-render rather than per-note, because the dynamics between notes are
/// carried from the speaker's energy envelope and levelling them would throw
/// away a measurement.
fn normalise(out: &mut [f32]) {
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak <= 0.0 {
        return;
    }
    let gain = HEADROOM / peak;
    for sample in out {
        *sample *= gain;
    }
}
