//! Per-frame loudness.

use music_analysis::energy::{SILENCE_DB, to_db, track};

#[test]
fn full_scale_is_zero_db() {
    assert!(to_db(1.0).abs() < 1e-6);
}

#[test]
fn half_amplitude_is_about_minus_six_db() {
    assert!((to_db(0.5) + 6.02).abs() < 0.01);
}

#[test]
fn silence_is_floored_not_infinite() {
    // Negative infinity serialises to `null` in JSON and poisons every plot and
    // average downstream.
    assert_eq!(to_db(0.0), SILENCE_DB);
    assert!(to_db(1e-30).is_finite());
}

#[test]
fn a_louder_signal_reads_louder() {
    let quiet = track(&vec![0.1f32; 1_600]);
    let loud = track(&vec![0.9f32; 1_600]);
    // Compare mid-signal frames; the first and last are edge-padded.
    assert!(loud[5] > quiet[5] + 15.0, "{} vs {}", loud[5], quiet[5]);
}

#[test]
fn empty_input_has_no_frames() {
    assert!(track(&[]).is_empty());
}
