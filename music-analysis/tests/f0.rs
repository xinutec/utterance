//! Pitch tracking: the right fundamental, and an honest refusal when there isn't one.

mod common;

use common::{noise, saw};
use music_analysis::f0::{F0Frame, track};

fn median_hz(frames: &[F0Frame]) -> f32 {
    let mut hz: Vec<f32> = frames.iter().filter_map(|f| f.hz).collect();
    assert!(!hz.is_empty(), "no voiced frames");
    hz.sort_by(f32::total_cmp);
    hz[hz.len() / 2]
}

fn voiced_fraction(frames: &[F0Frame]) -> f32 {
    frames.iter().filter(|f| f.hz.is_some()).count() as f32 / frames.len() as f32
}

#[test]
fn tracks_a_low_voice() {
    let got = median_hz(&track(&saw(85.0, 0.5)));
    assert!((got - 85.0).abs() < 1.0, "got {got}");
}

#[test]
fn tracks_a_high_voice() {
    let got = median_hz(&track(&saw(300.0, 0.5)));
    assert!((got - 300.0).abs() < 3.0, "got {got}");
}

#[test]
fn does_not_halve_the_pitch() {
    // The classic YIN failure: reporting 110 Hz for a 220 Hz tone because 2T is
    // also a period. Guarded by taking the first sub-threshold dip, not the
    // deepest one.
    let got = median_hz(&track(&saw(220.0, 0.5)));
    assert!((got - 220.0).abs() < 2.0, "got {got}");
}

#[test]
fn silence_is_unvoiced() {
    assert!(track(&vec![0.0f32; 8_000]).iter().all(|f| f.hz.is_none()));
}

#[test]
fn white_noise_is_unvoiced() {
    let fraction = voiced_fraction(&track(&noise(0.5)));
    assert!(
        fraction < 0.05,
        "{fraction:.2} of noise frames called voiced"
    );
}

#[test]
fn a_periodic_signal_reads_as_strongly_periodic() {
    let frames = track(&saw(150.0, 0.3));
    let mean = frames.iter().map(|f| f.aperiodicity).sum::<f32>() / frames.len() as f32;
    assert!(mean < 0.2, "periodic signal read as aperiodic: {mean}");
}

#[test]
fn aperiodicity_is_reported_even_where_unvoiced() {
    // The continuous measurement stays available on every frame; `hz` is only
    // that measurement thresholded, and a caller may want the raw number.
    let frames = track(&noise(0.2));
    assert!(frames.iter().all(|f| f.aperiodicity.is_finite()));
    assert!(frames.iter().all(|f| (0.0..=1.0).contains(&f.aperiodicity)));
}
