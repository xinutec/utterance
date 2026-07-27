//! Harmonic measurement against signals whose spectrum is known by construction.
//!
//! Every signal here is built from explicit sinusoids, so the answer is not a
//! matter of judgement: harmonic *k* is present at exactly the amplitude it was
//! put there with, or the measurement is wrong.

use music_analysis::f0;
use music_analysis::frame;
use music_analysis::partials::{self, MAX_PARTIAL};
use music_analysis::resample::ANALYSIS_RATE;

/// A tone built from named harmonics: `(number, amplitude)`.
///
/// Phases are all zero, which is fine and deliberate — a magnitude spectrum is
/// blind to phase, so nothing here depends on the choice.
fn harmonic_tone(f0_hz: f32, harmonics: &[(u32, f32)], secs: f32) -> Vec<f32> {
    let n = (ANALYSIS_RATE as f32 * secs) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / ANALYSIS_RATE as f32;
            harmonics
                .iter()
                .map(|&(k, a)| a * (2.0 * std::f32::consts::PI * f0_hz * k as f32 * t).sin())
                .sum::<f32>()
                * 0.2
        })
        .collect()
}

/// Measure a signal the way `analyse` does — pitch track first, then partials.
fn measure(samples: &[f32]) -> partials::Partials {
    let pitch: Vec<Option<f32>> = f0::track(samples).iter().map(|f| f.hz).collect();
    partials::measure(samples, &pitch)
}

/// The amplitude reported for harmonic `k`, if it was reported at all.
fn amplitude_of(p: &partials::Partials, k: u32) -> Option<f32> {
    p.partials
        .iter()
        .find(|x| x.number == k)
        .map(|x| x.amplitude)
}

#[test]
fn finds_the_harmonics_that_are_there_and_no_others() {
    // Odd harmonics only, as a square-ish wave has. If the measurement invents
    // the even ones, the presence gate is not doing its job.
    let signal = harmonic_tone(150.0, &[(1, 1.0), (3, 0.5), (5, 0.3), (7, 0.2)], 2.0);
    let p = measure(&signal);

    for k in [1, 3, 5, 7] {
        assert!(amplitude_of(&p, k).is_some(), "harmonic {k} was not found");
    }
    for k in [2, 4, 6, 8] {
        assert!(
            amplitude_of(&p, k).is_none(),
            "harmonic {k} was reported but was never in the signal"
        );
    }
}

#[test]
fn recovers_the_amplitude_profile() {
    // The profile is the payload: a tuning derived from this spectrum depends on
    // these ratios between partials, not on their absolute level.
    let signal = harmonic_tone(140.0, &[(1, 1.0), (2, 0.5), (3, 0.25), (4, 0.125)], 2.0);
    let p = measure(&signal);

    let first = amplitude_of(&p, 1).expect("fundamental");
    assert!(
        (first - 1.0).abs() < 0.01,
        "loudest partial should normalise to 1, got {first}"
    );
    for (k, expected) in [(2u32, 0.5f32), (3, 0.25), (4, 0.125)] {
        let got = amplitude_of(&p, k).unwrap_or_else(|| panic!("harmonic {k} missing"));
        assert!(
            (got - expected).abs() < 0.05,
            "harmonic {k}: expected about {expected}, measured {got}"
        );
    }
}

#[test]
fn measures_a_quiet_partial_between_loud_ones() {
    // The window's sidelobes are what this is really testing: harmonics 1 and 3
    // are 30 dB above harmonic 2, and leakage from either would fill 2's bins
    // and report it far louder than it is.
    let signal = harmonic_tone(160.0, &[(1, 1.0), (2, 0.03), (3, 1.0)], 2.0);
    let p = measure(&signal);

    let quiet = amplitude_of(&p, 2).expect("the quiet harmonic should still be found");
    assert!(
        quiet < 0.1,
        "harmonic 2 is 30 dB down but measured {quiet} — spectral leakage is being read as signal"
    );
}

#[test]
fn reports_ratios_close_to_whole_numbers() {
    // A synthesised harmonic tone really is harmonic, so this is a check on the
    // measurement rather than a discovery about the signal. Parabolic
    // interpolation is what makes it possible: without it a ratio is quantised
    // to the bin spacing, which at this f0 is several percent.
    let signal = harmonic_tone(
        130.0,
        &[(1, 1.0), (2, 0.8), (3, 0.6), (4, 0.4), (5, 0.3)],
        2.0,
    );
    let p = measure(&signal);

    for partial in &p.partials {
        let deviation = (partial.ratio - partial.number as f32).abs();
        assert!(
            deviation < 0.02,
            "harmonic {} measured at ratio {:.4}",
            partial.number,
            partial.ratio
        );
    }
}

#[test]
fn reports_the_fundamental_it_measured_against() {
    let signal = harmonic_tone(200.0, &[(1, 1.0), (2, 0.5)], 2.0);
    let p = measure(&signal);
    let f0 = p.f0_hz.expect("a tone has a fundamental");
    assert!((f0 - 200.0).abs() < 4.0, "f0 measured as {f0}");
}

#[test]
fn uses_most_of_a_steady_take() {
    let signal = harmonic_tone(150.0, &[(1, 1.0), (2, 0.5)], 2.0);
    let p = measure(&signal);
    let total = frame::count(signal.len());
    assert!(
        p.frames_used > total / 2,
        "only {} of {total} frames were usable on a perfectly steady tone",
        p.frames_used
    );
}

#[test]
fn refuses_a_signal_whose_pitch_will_not_hold_still() {
    // A tone sweeping across an octave has no single harmonic series. The right
    // answer is to use almost nothing, not to average the sweep into a
    // confident-looking spectrum.
    let n = (ANALYSIS_RATE as f32 * 2.0) as usize;
    let mut phase = 0.0f32;
    let sweep: Vec<f32> = (0..n)
        .map(|i| {
            let hz = 120.0 * 2f32.powf(i as f32 / n as f32);
            phase += 2.0 * std::f32::consts::PI * hz / ANALYSIS_RATE as f32;
            (phase.sin() + 0.5 * (2.0 * phase).sin()) * 0.2
        })
        .collect();

    let steady = measure(&harmonic_tone(120.0, &[(1, 1.0), (2, 0.5)], 2.0));
    let swept = measure(&sweep);
    assert!(
        swept.frames_used * 2 < steady.frames_used,
        "a sweep used {} frames against a steady tone's {}",
        swept.frames_used,
        steady.frames_used
    );
}

#[test]
fn says_nothing_about_silence() {
    let p = measure(&vec![0.0; ANALYSIS_RATE as usize]);
    assert_eq!(p.frames_used, 0);
    assert!(p.partials.is_empty());
    assert!(p.f0_hz.is_none());
}

#[test]
fn never_reports_a_harmonic_past_its_ceiling() {
    let signal = harmonic_tone(
        120.0,
        &(1..=40).map(|k| (k, 1.0 / k as f32)).collect::<Vec<_>>(),
        2.0,
    );
    let p = measure(&signal);
    assert!(p.partials.iter().all(|x| x.number as usize <= MAX_PARTIAL));
    assert!(!p.partials.is_empty(), "a rich tone should yield partials");
}

#[test]
fn is_a_pure_function_of_its_input() {
    let signal = harmonic_tone(145.0, &[(1, 1.0), (2, 0.6), (3, 0.3)], 1.5);
    let a = measure(&signal);
    let b = measure(&signal);
    assert_eq!(a.partials, b.partials);
    assert_eq!(a.frames_used, b.frames_used);
}
