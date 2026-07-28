//! Speaker profiling against real recorded audio.
//!
//! The fixture is one speaker gliding *ee → ah → oo* — see
//! `tests/fixtures/README.md`. Those three vowels are close to the corners of a
//! vowel space, so a profile built from it should come out near the published
//! ranges for an adult speaker. That is a weak assertion by design: this fixture
//! is a calibration take of one person, and what is being checked is that the
//! profile lands somewhere anatomically possible rather than that it is right
//! about this person. Nobody has measured their real corners.

use utterance_analysis::analyse_wav;
use utterance_analysis::speaker;

/// A real glided vowel — see `tests/fixtures/README.md`.
const GLIDE: &[u8] = include_bytes!("fixtures/sustained-vowel.wav");

#[test]
fn profiles_a_real_calibration_take() {
    let vp = analyse_wav(GLIDE).expect("fixture analyses");
    let p = speaker::profile(&[&vp]);

    let space = p
        .vowel_space
        .expect("seven seconds of phonation is enough for a vowel space");
    let f0 = p.f0.expect("a sustained vowel is voiced");

    // Anatomical bounds for an adult speaker, wide on purpose.
    assert!(
        (200.0..500.0).contains(&space.f1_low) && (400.0..1000.0).contains(&space.f1_high),
        "F1 range implausible: {space:?}"
    );
    assert!(
        (600.0..1400.0).contains(&space.f2_low) && (1400.0..2800.0).contains(&space.f2_high),
        "F2 range implausible: {space:?}"
    );

    // The glide runs ee → ah → oo, so it visits both ends of both axes. A space
    // that collapsed to a point would mean the tracker followed only one vowel.
    assert!(
        space.f1_high - space.f1_low > 100.0,
        "F1 barely moved across a three-vowel glide: {space:?}"
    );
    assert!(
        space.f2_high - space.f2_low > 300.0,
        "F2 barely moved across a three-vowel glide: {space:?}"
    );

    // Held at a near-constant pitch, confirmed with the speaker.
    assert!(
        (100.0..180.0).contains(&f0.median_hz),
        "median f0 {} outside the pitch this take was sung at",
        f0.median_hz
    );
    assert!(
        f0.high_hz - f0.low_hz < 60.0,
        "a deliberately steady pitch should not span {} Hz",
        f0.high_hz - f0.low_hz
    );
}

#[test]
fn is_a_pure_function_of_its_input() {
    let vp = analyse_wav(GLIDE).expect("fixture analyses");
    let a = speaker::profile(&[&vp]);
    let b = speaker::profile(&[&vp]);
    assert_eq!(a.vowel_space, b.vowel_space);
    assert_eq!(a.f0, b.f0);
}

#[test]
fn every_measured_frame_lands_near_the_normalised_unit_square() {
    let vp = analyse_wav(GLIDE).expect("fixture analyses");
    let space = speaker::profile(&[&vp]).vowel_space.unwrap();

    let points = vp.formants.vowel_space();
    let outside = points
        .iter()
        .filter(|(f1, f2)| {
            let (x, y) = space.normalise(*f1, *f2);
            !(-0.001..=1.001).contains(&x) || !(-0.001..=1.001).contains(&y)
        })
        .count();

    // The bounds trim 5% from each end of each axis, so up to ~20% of frames can
    // fall outside the unit square on one axis or the other. Far more than that
    // means the percentiles are not describing this take.
    let fraction = outside as f32 / points.len() as f32;
    assert!(
        fraction < 0.25,
        "{:.0}% of frames outside the speaker's own space",
        fraction * 100.0
    );
}
