//! Event detection.

mod common;

use common::bursts;
use utterance_analysis::frame::HOP;
use utterance_analysis::onset::{flux, pick};
use utterance_analysis::resample::ANALYSIS_RATE;

fn onset_times(x: &[f32]) -> Vec<f32> {
    pick(&flux(x))
        .into_iter()
        .map(|i| (i * HOP) as f32 / ANALYSIS_RATE as f32)
        .collect()
}

#[test]
fn finds_isolated_bursts() {
    let times = onset_times(&bursts(&[0.2, 0.6, 1.0], 1.5));
    assert_eq!(times.len(), 3, "got {times:?}");
    for (got, want) in times.iter().zip([0.2, 0.6, 1.0]) {
        assert!((got - want).abs() < 0.03, "onset at {got}, expected {want}");
    }
}

#[test]
fn a_sound_stopping_is_not_an_onset() {
    // Truncating a steady tone widens its mainlobe, so neighbouring bins gain
    // magnitude and half-wave-rectified flux spikes — at the moment the sound
    // *ends*. Each burst must report one event, not two.
    let times = onset_times(&bursts(&[0.3], 1.0));
    assert_eq!(times.len(), 1, "got {times:?}");
}

#[test]
fn silence_has_no_onsets() {
    assert!(onset_times(&vec![0.0f32; ANALYSIS_RATE as usize]).is_empty());
}

#[test]
fn a_steady_tone_onsets_once() {
    // Only the attack is an event; the sustain must not keep firing.
    let n = ANALYSIS_RATE as usize;
    let x: Vec<f32> = (0..n)
        .map(|i| {
            if i < n / 4 {
                0.0
            } else {
                (2.0 * std::f32::consts::PI * 300.0 * (i as f32) / ANALYSIS_RATE as f32).sin() * 0.8
            }
        })
        .collect();
    assert_eq!(onset_times(&x).len(), 1, "{:?}", onset_times(&x));
}

#[test]
fn empty_input_is_handled() {
    assert!(flux(&[]).is_empty());
    assert!(pick(&[]).is_empty());
}

#[test]
fn flux_is_normalised() {
    let f = flux(&bursts(&[0.2, 0.6], 1.0));
    assert!(f.iter().all(|v| (0.0..=1.0).contains(v)));
    assert!((f.iter().copied().fold(0.0f32, f32::max) - 1.0).abs() < 1e-6);
}
