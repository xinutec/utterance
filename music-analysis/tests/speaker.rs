//! Speaker profiling over voiceprints built by hand.
//!
//! Hand-built rather than analysed, because what is under test here is the
//! statistics — which percentiles are taken, when a range is withheld, how the
//! normalisation maps back — and feeding it real audio would make every
//! assertion depend on the formant tracker as well. `speaker_real.rs` next door
//! covers the same code against a recording.

use music_analysis::speaker::{self, PROFILE_VERSION};
use music_analysis::voiceprint::{Events, Formants, FrameGrid, Pitch, Source, Voiceprint};

/// A voiceprint carrying the given per-frame series and nothing else of interest.
fn voiceprint(f1: Vec<Option<f32>>, f2: Vec<Option<f32>>, hz: Vec<Option<f32>>) -> Voiceprint {
    let count = f1.len().max(hz.len());
    Voiceprint {
        schema_version: 0,
        source: Source {
            sample_rate_hz: 16_000,
            channels: 1,
            duration_s: count as f32 / 100.0,
            peak: 0.5,
            clipped_fraction: 0.0,
        },
        frame: FrameGrid {
            analysis_rate_hz: 16_000,
            hop_s: 0.01,
            count,
        },
        pitch: Pitch {
            aperiodicity: vec![0.1; hz.len()],
            hz,
        },
        formants: Formants {
            f3: vec![None; f1.len()],
            f1,
            f2,
        },
        rms_db: vec![-20.0; count],
        events: Events {
            flux: vec![0.0; count],
            onset_frames: Vec::new(),
            onset_times_s: Vec::new(),
        },
    }
}

/// `n` frames ramping linearly from `from` to `to`.
fn ramp(from: f32, to: f32, n: usize) -> Vec<Option<f32>> {
    (0..n)
        .map(|i| Some(from + (to - from) * i as f32 / (n - 1) as f32))
        .collect()
}

/// A speaker who sweeps both formants and their pitch across a known range.
fn sweeping_speaker(frames: usize) -> Voiceprint {
    voiceprint(
        ramp(300.0, 800.0, frames),
        ramp(900.0, 2400.0, frames),
        ramp(100.0, 200.0, frames),
    )
}

#[test]
fn reports_the_percentile_edges_not_the_extremes() {
    let vp = sweeping_speaker(1001);
    let p = speaker::profile(&[&vp]);

    // A linear ramp puts the 5th percentile exactly 5% along it.
    let space = p.vowel_space.expect("enough frames for a vowel space");
    assert!(
        (space.f1_low - 325.0).abs() < 1.0,
        "f1_low {}",
        space.f1_low
    );
    assert!(
        (space.f1_high - 775.0).abs() < 1.0,
        "f1_high {}",
        space.f1_high
    );
    assert!(
        (space.f2_low - 975.0).abs() < 1.0,
        "f2_low {}",
        space.f2_low
    );
    assert!(
        (space.f2_high - 2325.0).abs() < 1.0,
        "f2_high {}",
        space.f2_high
    );

    let f0 = p.f0.expect("enough voiced frames");
    assert!((f0.low_hz - 105.0).abs() < 1.0, "low {}", f0.low_hz);
    assert!(
        (f0.median_hz - 150.0).abs() < 1.0,
        "median {}",
        f0.median_hz
    );
    assert!((f0.high_hz - 195.0).abs() < 1.0, "high {}", f0.high_hz);
}

#[test]
fn one_wild_frame_does_not_define_the_space() {
    let clean = sweeping_speaker(1001);
    let mut with_outlier = sweeping_speaker(1001);
    // The failure this guards: per-frame formant assignment misfires on a few
    // frames per take, and a min/max bound would hand the whole space to them.
    with_outlier.formants.f2[500] = Some(9_000.0);

    let a = speaker::profile(&[&clean]).vowel_space.unwrap();
    let b = speaker::profile(&[&with_outlier]).vowel_space.unwrap();
    assert!(
        (a.f2_high - b.f2_high).abs() < 5.0,
        "outlier moved f2_high from {} to {}",
        a.f2_high,
        b.f2_high
    );
}

#[test]
fn withholds_a_range_it_cannot_measure() {
    let thin = sweeping_speaker(50);
    let p = speaker::profile(&[&thin]);
    assert!(p.vowel_space.is_none(), "50 frames is not a vowel space");
    assert!(p.f0.is_none(), "50 frames is not a pitch range");
    // The counts are still reported, so a caller can tell "too little material"
    // apart from "no material".
    assert_eq!(p.vowel_frames, 50);
    assert_eq!(p.voiced_frames, 50);
}

#[test]
fn withholds_a_vowel_space_when_the_speaker_never_moved() {
    let flat = voiceprint(
        vec![Some(500.0); 500],
        vec![Some(1500.0); 500],
        vec![Some(120.0); 500],
    );
    let p = speaker::profile(&[&flat]);
    assert!(
        p.vowel_space.is_none(),
        "a zero-span space cannot be normalised into"
    );
    // Pitch is a different question: a monotone is a real, usable range of zero
    // width, and nothing divides by it.
    assert!(p.f0.is_some(), "flat pitch is still a measured pitch");
}

#[test]
fn counts_only_frames_with_both_formants() {
    let mut vp = sweeping_speaker(1001);
    for slot in vp.formants.f2.iter_mut().take(400) {
        *slot = None;
    }
    let p = speaker::profile(&[&vp]);
    assert_eq!(
        p.vowel_frames, 601,
        "half-known frames are not vowel frames"
    );
    assert_eq!(p.voiced_frames, 1001, "pitch is unaffected");
}

#[test]
fn pools_frames_across_takes() {
    let low = voiceprint(
        ramp(300.0, 400.0, 500),
        ramp(900.0, 1000.0, 500),
        ramp(100.0, 110.0, 500),
    );
    let high = voiceprint(
        ramp(700.0, 800.0, 500),
        ramp(2300.0, 2400.0, 500),
        ramp(190.0, 200.0, 500),
    );

    let together = speaker::profile(&[&low, &high]);
    assert_eq!(together.takes, 2);
    let space = together.vowel_space.unwrap();
    assert!(
        space.f1_low < 350.0 && space.f1_high > 750.0,
        "both takes should widen the space: {space:?}"
    );
}

#[test]
fn normalises_the_edges_to_zero_and_one() {
    let vp = sweeping_speaker(1001);
    let space = speaker::profile(&[&vp]).vowel_space.unwrap();

    let (x, y) = space.normalise(space.f1_low, space.f2_low);
    assert!(x.abs() < 1e-4 && y.abs() < 1e-4, "low corner ({x}, {y})");

    let (x, y) = space.normalise(space.f1_high, space.f2_high);
    assert!(
        (x - 1.0).abs() < 1e-4 && (y - 1.0).abs() < 1e-4,
        "high corner ({x}, {y})"
    );
}

#[test]
fn does_not_clamp_a_frame_past_the_speakers_reach() {
    let vp = sweeping_speaker(1001);
    let space = speaker::profile(&[&vp]).vowel_space.unwrap();

    // The bounds are percentiles, so real frames sit outside them. A mapping
    // needs to see that rather than have it folded to the edge here.
    let (x, _) = space.normalise(space.f1_high + (space.f1_high - space.f1_low), 1500.0);
    assert!(x > 1.9, "expected an out-of-range position, got {x}");
}

#[test]
fn stamps_the_profile_version() {
    let vp = sweeping_speaker(1001);
    assert_eq!(speaker::profile(&[&vp]).profile_version, PROFILE_VERSION);
}

#[test]
fn survives_having_nothing_to_measure() {
    let p = speaker::profile(&[]);
    assert_eq!(p.takes, 0);
    assert_eq!(p.vowel_frames, 0);
    assert!(p.vowel_space.is_none() && p.f0.is_none());
}
