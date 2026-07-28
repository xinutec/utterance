//! Sample-rate conversion: does it preserve what is in band and reject what is not.

mod common;

use common::{cycles, rms, sine};
use utterance_analysis::resample::{ANALYSIS_RATE, resample, to_mono};

#[test]
fn equal_rates_are_an_exact_noop() {
    let x = sine(440.0, 48_000, 0.05);
    assert_eq!(resample(&x, 48_000, 48_000), x);
}

#[test]
fn downsampling_preserves_frequency() {
    // 1 second of 440 Hz at 48k -> 16k should still be 440 cycles.
    let y = resample(&sine(440.0, 48_000, 1.0), 48_000, ANALYSIS_RATE);
    assert_eq!(y.len(), 16_000);
    // Allow one cycle of slop at the truncated edges.
    let c = cycles(&y);
    assert!((439..=441).contains(&c), "expected ~440 cycles, got {c}");
}

#[test]
fn upsampling_preserves_frequency() {
    let y = resample(&sine(200.0, 16_000, 1.0), 16_000, 48_000);
    assert_eq!(y.len(), 48_000);
    let c = cycles(&y);
    assert!((199..=201).contains(&c), "expected ~200 cycles, got {c}");
}

#[test]
fn downsampling_rejects_content_above_the_new_nyquist() {
    // 7 kHz survives a 48k -> 16k conversion (under the 8 kHz Nyquist); 12 kHz
    // must be filtered out rather than folded back to 4 kHz.
    let keep = resample(&sine(7_000.0, 48_000, 0.5), 48_000, ANALYSIS_RATE);
    let dropped = resample(&sine(12_000.0, 48_000, 0.5), 48_000, ANALYSIS_RATE);
    assert!(
        rms(&keep) > 0.5,
        "in-band tone was attenuated: {}",
        rms(&keep)
    );
    assert!(
        rms(&dropped) < 0.02,
        "out-of-band tone aliased through: {}",
        rms(&dropped)
    );
}

#[test]
fn an_empty_signal_resamples_to_nothing() {
    assert!(resample(&[], 48_000, ANALYSIS_RATE).is_empty());
}

#[test]
fn mono_mixdown_averages_channels() {
    let stereo = [1.0, 0.0, 0.5, 0.5, -1.0, 1.0];
    assert_eq!(to_mono(&stereo, 2), vec![0.5, 0.5, 0.0]);
}

#[test]
fn mono_input_passes_through_the_mixdown() {
    let mono = [0.1f32, -0.2, 0.3];
    assert_eq!(to_mono(&mono, 1), mono.to_vec());
}
