//! Rendering, checked against what the score asked for.
//!
//! Nothing here judges how it sounds. What it can check is that the synthesiser
//! plays what the score says and adds nothing of its own: the right frequency,
//! for the right length, at the right moment, without clicks and without
//! aliasing.

use music_mapping::score::{Event, NoiseEvent, Score};
use music_realisation::synth::{self, RENDER_RATE};
use music_realisation::wav;

/// A score with one fixed spectrum, so a test that is not about colour need not
/// mention it.
fn score(events: Vec<Event>, duration_s: f32, spectrum: Vec<f32>) -> Score {
    Score {
        duration_s,
        palette: vec![spectrum],
        detune_cents: 0.0,
        events,
        noise: Vec::new(),
    }
}

fn note(start_s: f32, duration_s: f32, hz: f32) -> Event {
    Event {
        start_s,
        duration_s,
        hz,
        amplitude: 1.0,
        colour_from: 0.0,
        colour_to: 0.0,
        breath: 0.0,
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
                amplitude: 1.0,
                ..note(0.0, 0.4, 300.0)
            },
            Event {
                amplitude: 0.25,
                ..note(0.5, 0.4, 300.0)
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

/// How bright a slice sounds, without an FFT.
///
/// The RMS of the signal's first difference over the RMS of the signal.
/// Differencing is a high-pass, so the ratio rises with energy at the top of the
/// spectrum. Counting zero crossings was tried first and is useless here: it
/// reports whichever partial is strongest and does not move at all under a tilt
/// that changes every partial's level but not their ranking.
fn brightness(samples: &[f32]) -> f32 {
    let rms = |xs: &[f32]| (xs.iter().map(|v| v * v).sum::<f32>() / xs.len().max(1) as f32).sqrt();
    let slope: Vec<f32> = samples.windows(2).map(|w| w[1] - w[0]).collect();
    let level = rms(samples);
    if level <= 0.0 {
        0.0
    } else {
        rms(&slope) / level
    }
}

/// A dark spectrum and a bright one, for tests about colour.
fn dark() -> Vec<f32> {
    vec![1.0, 0.3, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0]
}
fn bright() -> Vec<f32> {
    vec![0.05, 0.1, 0.2, 0.4, 0.7, 1.0, 0.7, 0.4]
}

#[test]
fn a_note_changes_colour_across_its_length() {
    // The reason the score carries two colours. A spectrum that holds still is
    // the dead-organ sound the whole widening was for.
    let s = Score {
        duration_s: 2.0,
        palette: vec![dark(), bright()],
        detune_cents: 0.0,
        noise: Vec::new(),
        events: vec![Event {
            colour_from: 0.0,
            colour_to: 1.0,
            ..note(0.0, 2.0, 200.0)
        }],
    };
    let rendered = synth::render(&s);
    let quarter = rendered.len() / 4;

    let start = brightness(&rendered[..quarter]);
    let end = brightness(&rendered[2 * quarter..3 * quarter]);
    assert!(
        end > start * 1.5,
        "colour did not travel: brightness {start:.4} at the start, {end:.4} later"
    );
}

#[test]
fn a_note_darkens_as_it_decays() {
    // Damping rises with frequency in every real resonator, so the attack is the
    // brightest moment. Without it a decaying note keeps its attack brightness
    // all the way down, which reads as synthetic long before anyone says why.
    let s = score(vec![note(0.0, 2.0, 150.0)], 2.0, bright());
    let rendered = synth::render(&s);
    let fifth = rendered.len() / 5;

    let early = brightness(&rendered[fifth / 2..fifth]);
    let late = brightness(&rendered[3 * fifth..4 * fifth]);
    assert!(
        late < early,
        "the tone did not darken: brightness {early:.4} early, {late:.4} late"
    );
}

#[test]
fn breath_puts_noise_in_the_tone() {
    // Silence between the harmonics is what makes pure additive synthesis sound
    // sterile. A breathy note must carry energy where no partial is.
    let pitched = score(vec![note(0.0, 1.0, 200.0)], 1.0, dark());
    let breathy = Score {
        events: vec![Event {
            breath: 0.6,
            ..note(0.0, 1.0, 200.0)
        }],
        ..score(Vec::new(), 1.0, dark())
    };

    // Noise crosses zero far more often than a tone of the same pitch does.
    let clean = brightness(&synth::render(&pitched));
    let noisy = brightness(&synth::render(&breathy));
    assert!(
        noisy > clean * 2.0,
        "breath added no noise: brightness {clean:.4} clean vs {noisy:.4} breathy"
    );
}

#[test]
fn detune_pulls_partials_off_their_exact_harmonics() {
    // Perfectly locked partials are what a computer makes and nothing else does.
    // Detuned ones beat against each other, so the envelope of a sustained note
    // stops being flat.
    let flat = score(vec![note(0.0, 2.0, 200.0)], 2.0, bright());
    let detuned = Score {
        detune_cents: 10.0,
        ..score(vec![note(0.0, 2.0, 200.0)], 2.0, bright())
    };

    let spread = |samples: Vec<f32>| {
        // Peak amplitude per 50 ms window; beating makes these vary.
        let window = RENDER_RATE as usize / 20;
        let peaks: Vec<f32> = samples
            .chunks(window)
            .map(|c| c.iter().fold(0.0f32, |m, s| m.max(s.abs())))
            .collect();
        let mean = peaks.iter().sum::<f32>() / peaks.len() as f32;
        peaks.iter().map(|p| (p - mean).abs()).sum::<f32>() / peaks.len() as f32
    };

    assert!(
        spread(synth::render(&detuned)) > spread(synth::render(&flat)),
        "detune produced no beating"
    );
}

#[test]
fn a_palette_of_one_still_renders() {
    // The state a speaker is in after a single calibration take. It should sound
    // like the old fixed timbre, not like silence.
    let s = score(vec![note(0.0, 0.5, 300.0)], 0.5, dark());
    let peak = synth::render(&s).iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(
        peak > 0.5,
        "a one-entry palette rendered at a peak of {peak}"
    );
}

#[test]
fn an_empty_palette_renders_silence_rather_than_guessing() {
    // Inventing a spectrum would put energy where the speaker's tract put none,
    // and report success while doing it.
    let s = Score {
        duration_s: 1.0,
        palette: Vec::new(),
        detune_cents: 0.0,
        events: vec![note(0.0, 0.5, 300.0)],
        noise: Vec::new(),
    };
    assert!(synth::render(&s).iter().all(|&v| v == 0.0));
}

fn noise_event(start_s: f32, duration_s: f32, centre_hz: f32, bandwidth_hz: f32) -> NoiseEvent {
    NoiseEvent {
        start_s,
        duration_s,
        centre_hz,
        bandwidth_hz,
        amplitude: 1.0,
    }
}

fn noise_score(events: Vec<NoiseEvent>, duration_s: f32) -> Score {
    Score {
        noise: events,
        ..score(Vec::new(), duration_s, vec![1.0])
    }
}

#[test]
fn a_consonant_sounds_where_the_score_puts_it() {
    let s = noise_score(vec![noise_event(0.5, 0.2, 5000.0, 3000.0)], 1.0);
    let rendered = synth::render(&s);
    let energy = |from: f32, to: f32| {
        let range = (from * RENDER_RATE as f32) as usize..(to * RENDER_RATE as f32) as usize;
        rendered[range].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    };
    assert!(energy(0.0, 0.45) < 1e-6, "sound before the consonant");
    assert!(energy(0.55, 0.65) > 0.1, "no sound during the consonant");
    assert!(energy(0.8, 1.0) < 1e-6, "sound after the consonant");
}

#[test]
fn a_bright_consonant_renders_brighter_than_a_dark_one() {
    // The whole point of measuring the centroid: the speaker's own s and sh must
    // come out as different sounds, not as one generic hiss.
    let ess = synth::render(&noise_score(
        vec![noise_event(0.0, 0.4, 7000.0, 3000.0)],
        0.5,
    ));
    let esh = synth::render(&noise_score(
        vec![noise_event(0.0, 0.4, 3000.0, 1500.0)],
        0.5,
    ));
    assert!(
        brightness(&ess) > brightness(&esh) * 1.3,
        "s {:.4} against sh {:.4}",
        brightness(&ess),
        brightness(&esh)
    );
}

#[test]
fn a_narrow_band_is_not_louder_than_a_wide_one() {
    // A resonator's gain rises sharply as its band narrows. Without the
    // compensation, a whistled consonant would arrive many times louder than an
    // airy one carrying the same measured energy.
    let narrow = synth::render(&noise_score(
        vec![noise_event(0.0, 0.4, 3000.0, 250.0)],
        0.5,
    ));
    let wide = synth::render(&noise_score(
        vec![noise_event(0.0, 0.4, 3000.0, 4000.0)],
        0.5,
    ));

    let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
    let ratio = rms(&narrow) / rms(&wide);
    assert!(
        (0.25..4.0).contains(&ratio),
        "narrow band rendered {ratio:.2} times the wide one's level"
    );
}

#[test]
fn notes_and_consonants_sound_together() {
    // They are separate streams in the score and must both reach the output —
    // the failure this guards is one silently overwriting the other.
    let both = Score {
        noise: vec![noise_event(0.0, 0.4, 6000.0, 3000.0)],
        ..score(vec![note(0.0, 0.4, 200.0)], 0.5, vec![1.0, 0.5])
    };
    let only_notes = score(vec![note(0.0, 0.4, 200.0)], 0.5, vec![1.0, 0.5]);

    assert!(
        brightness(&synth::render(&both)) > brightness(&synth::render(&only_notes)) * 1.2,
        "adding a consonant changed nothing about the render"
    );
}

#[test]
fn a_consonant_is_deterministic() {
    let s = noise_score(vec![noise_event(0.0, 0.3, 4000.0, 2000.0)], 0.5);
    assert_eq!(synth::render(&s), synth::render(&s));
}
