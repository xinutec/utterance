//! Periodicity per frequency band: breath is noise *above* a periodic voice, so
//! it shows in the high band and not the low one.

mod common;

use utterance_analysis::bands;
use utterance_analysis::frame;

/// The pitch track is given rather than estimated, so these test the band
/// measure and not YIN.
fn at(f0_hz: f32, samples: &[f32]) -> Vec<Option<f32>> {
    vec![Some(f0_hz); frame::count(samples.len())]
}

/// Mean over the frames clear of the edges, where the window is all signal.
fn middle(values: &[Option<f32>]) -> f32 {
    let inner: Vec<f32> = values[10..values.len() - 10]
        .iter()
        .flatten()
        .copied()
        .collect();
    assert!(!inner.is_empty(), "no measured frames");
    inner.iter().sum::<f32>() / inner.len() as f32
}

#[test]
fn a_sawtooth_is_periodic_in_both_bands() {
    let x = common::saw(150.0, 1.0);
    let b = bands::track(&x, &at(150.0, &x));
    assert!(middle(&b.low) < 0.1, "low {}", middle(&b.low));
    assert!(middle(&b.high) < 0.1, "high {}", middle(&b.high));
}

#[test]
fn breath_above_a_periodic_vowel_shows_only_in_the_high_band() {
    let vowel = common::vowel(150.0, 1.0);
    let x: Vec<f32> = vowel
        .iter()
        .zip(common::noise(1.0))
        .map(|(v, n)| v + 0.05 * n)
        .collect();
    let b = bands::track(&x, &at(150.0, &x));
    assert!(middle(&b.low) < 0.1, "low {}", middle(&b.low));
    assert!(middle(&b.high) > 0.5, "high {}", middle(&b.high));
}

#[test]
fn noise_is_aperiodic_in_both_bands() {
    let x = common::noise(1.0);
    let b = bands::track(&x, &at(150.0, &x));
    assert!(middle(&b.low) > 0.5, "low {}", middle(&b.low));
    assert!(middle(&b.high) > 0.5, "high {}", middle(&b.high));
}

#[test]
fn an_unvoiced_frame_has_no_measurement() {
    let x = common::saw(150.0, 0.5);
    let b = bands::track(&x, &vec![None; frame::count(x.len())]);
    assert!(b.low.iter().chain(&b.high).all(Option::is_none));
}
