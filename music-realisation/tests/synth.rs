//! Rendering, checked against what the score asked for.
//!
//! Nothing here judges how it sounds. What it can check is that the synthesiser
//! plays what the score says and adds nothing of its own: the right frequency,
//! for the right length, at the right moment, without clicks and without
//! aliasing.

use music_mapping::score::{Event, Score};
use music_realisation::synth::{self, RENDER_RATE};
use music_realisation::wav;

fn score(events: Vec<Event>, duration_s: f32, timbre: Vec<f32>) -> Score {
    Score {
        duration_s,
        timbre,
        events,
    }
}

fn note(start_s: f32, duration_s: f32, hz: f32) -> Event {
    Event {
        start_s,
        duration_s,
        hz,
        amplitude: 1.0,
    }
}

/// Zero crossings in the rising direction — a cheap frequency estimate that
/// needs no FFT and is exact enough for a pure tone.
fn rising_crossings(samples: &[f32]) -> usize {
    samples
        .windows(2)
        .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
        .count()
}

#[test]
fn renders_the_pitch_the_score_asked_for() {
    // A sine at 440 Hz for a second crosses zero upward 440 times.
    let s = score(vec![note(0.0, 1.0, 440.0)], 1.0, vec![1.0]);
    let rendered = synth::render(&s);
    let crossings = rising_crossings(&rendered);
    assert!(
        (crossings as i32 - 440).abs() <= 2,
        "expected about 440 cycles, counted {crossings}"
    );
}

#[test]
fn renders_a_pitch_no_sampled_instrument_could_play() {
    // The reason this crate is additive at all: 582 cents above 200 Hz is a
    // septimal tritone, and it has to come out exactly there.
    let hz = 200.0 * 2f32.powf(582.0 / 1200.0);
    let s = score(vec![note(0.0, 1.0, hz)], 1.0, vec![1.0]);
    let crossings = rising_crossings(&synth::render(&s)) as f32;
    assert!(
        (crossings - hz).abs() < 3.0,
        "asked for {hz:.1} Hz, rendered about {crossings:.0}"
    );
}

#[test]
fn places_a_note_where_the_score_puts_it() {
    let s = score(vec![note(1.0, 0.5, 440.0)], 2.0, vec![1.0]);
    let rendered = synth::render(&s);

    let silent = |from: f32, to: f32| {
        let range = (from * RENDER_RATE as f32) as usize..(to * RENDER_RATE as f32) as usize;
        rendered[range].iter().all(|s| s.abs() < 1e-6)
    };
    assert!(silent(0.0, 0.99), "sound before the note starts");
    assert!(silent(1.6, 2.0), "sound after the note ends");
    assert!(!silent(1.1, 1.4), "no sound during the note");
}

#[test]
fn starts_and_ends_without_a_click() {
    // A sinusoid switched on mid-cycle is a step, and a step is a click. The
    // envelope is the only thing preventing it, so this checks the edges rather
    // than the middle.
    let s = score(vec![note(0.0, 0.5, 300.0)], 0.5, vec![1.0]);
    let rendered = synth::render(&s);

    let biggest_step = rendered
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0f32, f32::max);
    // One cycle of a full-scale 300 Hz sine steps by at most 2*pi*300/44100.
    let smooth = std::f32::consts::TAU * 300.0 / RENDER_RATE as f32;
    assert!(
        biggest_step < smooth * 1.5,
        "largest sample-to-sample jump was {biggest_step:.4}, above the {smooth:.4} a clean tone gives"
    );
}

#[test]
fn refuses_to_alias_partials_above_nyquist() {
    // A rich timbre on a high note puts most of its harmonics past Nyquist. If
    // they were rendered they would fold back down as inharmonic noise, and be
    // heard as the tuning being wrong rather than as the synthesiser being wrong.
    let timbre: Vec<f32> = (1..=24).map(|k| 1.0 / k as f32).collect();
    let s = score(vec![note(0.0, 0.5, 5000.0)], 0.5, timbre);
    let rendered = synth::render(&s);

    // Everything that survives is a multiple of 5 kHz below 22.05 kHz, i.e.
    // 5, 10, 15 and 20 kHz — all well above the 1 kHz an alias would land near.
    let low_energy: f32 = rendered
        .windows(45)
        .step_by(45)
        .map(|w| w.iter().sum::<f32>().abs() / 45.0)
        .fold(0.0, f32::max);
    assert!(
        low_energy < 0.05,
        "energy appeared far below the fundamental: {low_energy:.3}"
    );
}

#[test]
fn keeps_the_dynamics_the_score_carried() {
    let s = score(
        vec![
            Event {
                start_s: 0.0,
                duration_s: 0.4,
                hz: 300.0,
                amplitude: 1.0,
            },
            Event {
                start_s: 0.5,
                duration_s: 0.4,
                hz: 300.0,
                amplitude: 0.25,
            },
        ],
        1.0,
        vec![1.0],
    );
    let rendered = synth::render(&s);
    let peak_in = |from: f32, to: f32| {
        let range = (from * RENDER_RATE as f32) as usize..(to * RENDER_RATE as f32) as usize;
        rendered[range].iter().fold(0.0f32, |m, s| m.max(s.abs()))
    };

    let ratio = peak_in(0.1, 0.3) / peak_in(0.6, 0.8);
    assert!(
        (ratio - 4.0).abs() < 0.4,
        "a note four times louder rendered {ratio:.2} times louder"
    );
}

#[test]
fn leaves_headroom() {
    let s = score(
        (0..8)
            .map(|i| note(i as f32 * 0.1, 0.5, 200.0 + 40.0 * i as f32))
            .collect(),
        1.5,
        vec![1.0, 0.5, 0.25],
    );
    let peak = synth::render(&s).iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(
        (0.8..1.0).contains(&peak),
        "overlapping notes rendered at a peak of {peak:.3}"
    );
}

#[test]
fn renders_silence_for_a_score_with_no_notes() {
    let rendered = synth::render(&score(Vec::new(), 1.0, vec![1.0]));
    assert_eq!(rendered.len(), RENDER_RATE as usize);
    assert!(rendered.iter().all(|&s| s == 0.0));
}

#[test]
fn is_a_pure_function_of_its_input() {
    let s = score(
        vec![note(0.0, 0.5, 440.0), note(0.3, 0.5, 660.0)],
        1.0,
        vec![1.0, 0.5],
    );
    assert_eq!(synth::render(&s), synth::render(&s));
}

#[test]
fn writes_a_wav_the_analyser_can_read_back() {
    // The loop closes here: anything this project renders must be something it
    // could also analyse, or the output is not really audio.
    let s = score(vec![note(0.0, 1.0, 440.0)], 1.0, vec![1.0]);
    let bytes = wav::encode(&synth::render(&s));

    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    assert_eq!(rate, RENDER_RATE);
}
