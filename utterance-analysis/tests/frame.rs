//! The frame grid every per-frame series is indexed by.

use utterance_analysis::frame::{HOP, count, hann, time_s, windowed};

#[test]
fn frame_times_advance_by_the_hop() {
    assert_eq!(time_s(0), 0.0);
    assert!((time_s(100) - 1.0).abs() < 1e-6);
}

#[test]
fn empty_input_has_no_frames() {
    assert_eq!(count(0), 0);
}

#[test]
fn frame_count_covers_a_partial_final_hop() {
    assert_eq!(count(HOP), 1);
    assert_eq!(count(HOP + 1), 2);
}

#[test]
fn windows_are_centred_on_their_frame() {
    let samples: Vec<f32> = (0..1000).map(|i| i as f32).collect();
    // Frame 2 starts at sample 320; a window of 8 spans 316..324.
    assert_eq!(
        windowed(&samples, 2, 8),
        vec![316.0, 317.0, 318.0, 319.0, 320.0, 321.0, 322.0, 323.0]
    );
}

#[test]
fn windows_zero_pad_at_the_edges() {
    assert_eq!(windowed(&[1.0, 2.0, 3.0], 0, 4), vec![0.0, 0.0, 1.0, 2.0]);
}

#[test]
fn hann_starts_at_zero_and_peaks_in_the_middle() {
    let w = hann(8);
    assert!(w[0].abs() < 1e-6);
    assert!((w[4] - 1.0).abs() < 1e-6);
}
