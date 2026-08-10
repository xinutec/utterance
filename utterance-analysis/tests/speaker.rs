//! Speaker profiling over voiceprints built by hand.
//!
//! Hand-built rather than analysed, because what is under test here is the
//! statistics — which percentiles are taken, when a range is withheld, how the
//! normalisation maps back — and feeding it real audio would make every
//! assertion depend on the formant tracker as well. `speaker_real.rs` next door
//! covers the same code against a recording.

use utterance_analysis::partials::Partials;
use utterance_analysis::speaker::{self, Brightness, PROFILE_VERSION};
use utterance_analysis::texture::Texture;
use utterance_analysis::voiceprint::{Events, Formants, FrameGrid, Pitch, Source, Voiceprint};

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
        // The harmonic series is not read by profiling, so it is left empty
        // rather than faked into plausibility. The centroid *is* read — for the
        // brightness range — and zero is how a frame says it carried no energy,
        // so a fixture that never sets it reports no brightness at all. The
        // tests that care set it themselves.
        partials: Partials {
            frames_used: 0,
            f0_hz: None,
            partials: Vec::new(),
        },
        texture: Texture {
            centroid_hz: vec![0.0; count],
            flatness: vec![0.0; count],
            tilt_db_per_octave: vec![0.0; count],
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

/// A speaker whose tone sweeps a known brightness range while voiced throughout.
fn brightening_speaker(frames: usize) -> Voiceprint {
    let mut vp = voiceprint(
        vec![Some(500.0); frames],
        vec![Some(1500.0); frames],
        vec![Some(120.0); frames],
    );
    // Geometric, because the range is read on a log axis and a linear ramp
    // would put the median somewhere the percentiles disagree with.
    vp.texture.centroid_hz = (0..frames)
        .map(|i| 400.0 * 4f32.powf(i as f32 / (frames - 1) as f32))
        .collect();
    vp
}

#[test]
fn brightness_spans_the_tone_a_speaker_actually_produced() {
    let vp = brightening_speaker(1000);
    let range = speaker::profile(&[&vp])
        .brightness
        .expect("a brightness range");

    // 400 Hz to 1600 Hz, trimmed a twentieth from each end on a log axis.
    assert!(
        (range.low_hz - 428.0).abs() < 15.0,
        "low edge was {}",
        range.low_hz
    );
    assert!(
        (range.high_hz - 1495.0).abs() < 40.0,
        "high edge was {}",
        range.high_hz
    );
}

#[test]
fn brightness_places_a_tone_within_that_range() {
    let vp = brightening_speaker(1000);
    let range = speaker::profile(&[&vp]).brightness.unwrap();

    // The geometric middle of the range, which is where a listener hears halfway.
    let middle = (range.low_hz * range.high_hz).sqrt();
    assert!(
        (range.place(middle) - 0.5).abs() < 0.01,
        "the perceptual midpoint landed at {}",
        range.place(middle)
    );
    assert!(range.place(range.low_hz).abs() < 0.001);
    assert!((range.place(range.high_hz) - 1.0).abs() < 0.001);
}

#[test]
fn an_unvoiced_frame_never_widens_the_brightness_range() {
    // Consonants are several times brighter than any sustained tone. Counted in,
    // they would stretch the top of the axis to somewhere no note ever reaches
    // and crowd every vowel into the bottom of a range describing sibilance.
    let mut vp = brightening_speaker(1000);
    for i in 0..200 {
        vp.pitch.hz[i] = None;
        vp.texture.centroid_hz[i] = 9000.0;
    }

    let range = speaker::profile(&[&vp]).brightness.unwrap();
    assert!(
        range.high_hz < 2000.0,
        "sibilance stretched the tone range to {}",
        range.high_hz
    );
}

#[test]
fn no_brightness_is_reported_from_too_few_voiced_frames() {
    // Same bar as the other ranges: a profile confidently reporting a range
    // measured over half a second is worse than one reporting nothing, because
    // a caller can handle an absence and cannot detect a wrong answer.
    let vp = brightening_speaker(100);
    assert!(speaker::profile(&[&vp]).brightness.is_none());
}

// ---- one held vowel: where a corner of the space actually is ----------------

/// A held vowel: `hold` frames parked on `(f1, f2)`, with `glide` frames on
/// either side sweeping in from and back out to a neutral centre.
///
/// The glide is the point of the fixture. A corner take is a person opening
/// their mouth into a shape and closing it again, so the first and last frames
/// are real measurements of something that is not the vowel being asked for.
fn held_vowel(f1: f32, f2: f32, hold: usize, glide: usize) -> Voiceprint {
    let mut a: Vec<Option<f32>> = ramp(500.0, f1, glide);
    let mut b: Vec<Option<f32>> = ramp(1500.0, f2, glide);
    a.extend(std::iter::repeat_n(Some(f1), hold));
    b.extend(std::iter::repeat_n(Some(f2), hold));
    a.extend(ramp(f1, 500.0, glide));
    b.extend(ramp(f2, 1500.0, glide));
    let count = a.len();
    voiceprint(a, b, vec![Some(120.0); count])
}

#[test]
fn a_corner_is_where_the_vowel_was_held() {
    // 300 frames of a close front vowel, 40 of gliding in and out of it.
    let corner = speaker::corner(&held_vowel(280.0, 2300.0, 300, 40)).unwrap();
    assert!(
        (corner.f1_hz - 280.0).abs() < 5.0 && (corner.f2_hz - 2300.0).abs() < 5.0,
        "held at (280, 2300), measured ({}, {})",
        corner.f1_hz,
        corner.f2_hz
    );
    assert_eq!(corner.frames, 380);
}

#[test]
fn the_glide_does_not_drag_the_corner_toward_neutral() {
    // The property the median is for. A mean over this fixture lands well short
    // of the vowel: 80 glide frames average halfway to the neutral centre, so
    // they pull F2 down by roughly (2300-1500)/2 * 80/380 ≈ 84 Hz — a tenth of
    // the distance from ee to the middle of the chart, in the direction of
    // making every corner look less extreme than the speaker actually is.
    let vp = held_vowel(280.0, 2300.0, 300, 40);
    let pairs = vp.formants.vowel_space();
    let mean_f2: f32 = pairs.iter().map(|(_, b)| b).sum::<f32>() / pairs.len() as f32;
    assert!(
        mean_f2 < 2260.0,
        "fixture is not exercising the difference: mean F2 is {mean_f2}"
    );

    let corner = speaker::corner(&vp).unwrap();
    assert!(
        corner.f2_hz > mean_f2 + 50.0,
        "median {} should sit at the held value, well above the mean {mean_f2}",
        corner.f2_hz
    );
}

#[test]
fn the_spread_says_whether_the_vowel_was_held_still() {
    // Two takes with the same centre. A dot on a chart cannot tell them apart,
    // which is why the spread is reported beside it.
    let steady = speaker::corner(&held_vowel(280.0, 2300.0, 300, 40)).unwrap();
    let wandering = speaker::corner(&voiceprint(
        ramp(180.0, 380.0, 380),
        ramp(2100.0, 2500.0, 380),
        vec![Some(120.0); 380],
    ))
    .unwrap();

    assert!(
        (wandering.f1_hz - steady.f1_hz).abs() < 15.0,
        "the fixtures are meant to share a centre: {} vs {}",
        wandering.f1_hz,
        steady.f1_hz
    );
    assert!(
        wandering.f2_spread_hz > steady.f2_spread_hz * 4.0,
        "a vowel that wandered 400 Hz reported a spread of {} against the held take's {}",
        wandering.f2_spread_hz,
        steady.f2_spread_hz
    );
}

#[test]
fn withholds_a_corner_it_cannot_measure() {
    // Half a second. Same reasoning as the ranges above: an absent corner is a
    // state the caller can show as "not recorded yet", and a corner measured
    // over fifty frames is one nobody can tell is wrong.
    assert!(speaker::corner(&held_vowel(280.0, 2300.0, 40, 5)).is_none());
}

#[test]
fn a_frame_missing_either_formant_is_not_a_point_on_the_plane() {
    // The corner is measured over the pairs, so a take whose F2 never resolved
    // has no corner at all — rather than one placed by F1 alone.
    let mut vp = held_vowel(280.0, 2300.0, 300, 40);
    vp.formants.f2 = vec![None; vp.formants.f2.len()];
    assert!(speaker::corner(&vp).is_none());
}

#[test]
fn a_brightness_range_that_runs_backwards_is_refused() {
    // `place` divides by `high_hz - low_hz` without a guard, which is only sound
    // because the constructor already refused the cases that make it wrong. An
    // inverted range would not divide by zero — it would divide by a negative,
    // and every brightness would come back mirrored: the darkest frame reported
    // as the brightest, silently and for the whole take.
    //
    // Dropping the `high_hz <= low_hz` half of that guard passed the entire
    // suite on 2026-08-07.
    assert!(
        Brightness::new(3000.0, 300.0).is_none(),
        "a range from 3000 Hz down to 300 Hz was accepted"
    );
    assert!(
        Brightness::new(1000.0, 1000.0).is_none(),
        "a range with no extent was accepted"
    );
    assert!(
        Brightness::new(300.0, 3000.0).is_some(),
        "the ordinary range was refused"
    );
}
