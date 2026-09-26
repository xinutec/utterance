//! Formant tracking against a real glide of known content, *ee → ah → oo*, whose
//! positions are the best-measured fact in phonetics: **F1 low, high, low; F2
//! high, mid, low.** A tracker that cannot reproduce that is not measuring vowels.

use utterance_analysis::analyse_wav;
use utterance_analysis::voiceprint::Voiceprint;

/// A real glided vowel — see `tests/fixtures/README.md`.
const GLIDE: &[u8] = include_bytes!("fixtures/sustained-vowel.wav");

/// Median of a formant series over `[from, to)` seconds.
fn median_over(vp: &Voiceprint, series: &[Option<f32>], from: f32, to: f32) -> f32 {
    let frame_of = |t: f32| (t / vp.frame.hop_s) as usize;
    let mut values: Vec<f32> = series[frame_of(from)..frame_of(to).min(series.len())]
        .iter()
        .flatten()
        .copied()
        .collect();
    assert!(!values.is_empty(), "no estimates between {from}s and {to}s");
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

/// The steady portion of each vowel, avoiding the transitions between them.
const EE: (f32, f32) = (0.5, 2.0);
const AH: (f32, f32) = (2.8, 4.2);
const OO: (f32, f32) = (4.6, 7.0);

#[test]
fn the_three_vowels_land_where_those_vowels_live() {
    let vp = analyse_wav(GLIDE).unwrap();
    let f1 = |w: (f32, f32)| median_over(&vp, &vp.formants.f1, w.0, w.1);
    let f2 = |w: (f32, f32)| median_over(&vp, &vp.formants.f2, w.0, w.1);

    // /i/ — close and front: the jaw nearly shut puts F1 low, the tongue forward
    // puts F2 very high.
    assert!(
        f1(EE) < 400.0,
        "ee has F1 = {:.0} Hz, too open for a close vowel",
        f1(EE)
    );
    assert!(
        f2(EE) > 1800.0,
        "ee has F2 = {:.0} Hz, too low for a front vowel",
        f2(EE)
    );

    // /ɑ/ — open and back: the jaw down raises F1, the tongue back lowers F2.
    assert!(
        f1(AH) > 550.0,
        "ah has F1 = {:.0} Hz, too closed for an open vowel",
        f1(AH)
    );
    assert!(
        (1_000.0..1_700.0).contains(&f2(AH)),
        "ah has F2 = {:.0} Hz",
        f2(AH)
    );

    // /u/ — close and back: both low.
    assert!(
        f1(OO) < 400.0,
        "oo has F1 = {:.0} Hz, too open for a close vowel",
        f1(OO)
    );
    assert!(
        f2(OO) < 1_100.0,
        "oo has F2 = {:.0} Hz, too high for a back vowel",
        f2(OO)
    );
}

#[test]
fn the_trajectory_has_the_shape_the_glide_describes() {
    // Relations rather than values, so they hold for any vocal tract.
    let vp = analyse_wav(GLIDE).unwrap();
    let f1 = |w: (f32, f32)| median_over(&vp, &vp.formants.f1, w.0, w.1);
    let f2 = |w: (f32, f32)| median_over(&vp, &vp.formants.f2, w.0, w.1);

    assert!(f1(AH) > f1(EE) + 200.0, "ah is not opener than ee");
    assert!(f1(AH) > f1(OO) + 200.0, "ah is not opener than oo");
    assert!(f2(EE) > f2(AH) + 400.0, "ee is not fronter than ah");
    assert!(f2(AH) > f2(OO) + 200.0, "ah is not fronter than oo");
}

#[test]
fn no_formant_is_reported_outside_its_anatomical_range() {
    // A missed formant shifts the rest down a slot, out of range.
    let vp = analyse_wav(GLIDE).unwrap();

    for &f1 in vp.formants.f1.iter().flatten() {
        assert!((200.0..=1_100.0).contains(&f1), "F1 reported at {f1:.0} Hz");
    }
    for &f2 in vp.formants.f2.iter().flatten() {
        assert!((600.0..=3_000.0).contains(&f2), "F2 reported at {f2:.0} Hz");
    }
}

#[test]
fn most_voiced_frames_yield_a_vowel_space_position() {
    // Nulling every doubtful frame would pass the test above and leave nothing.
    let vp = analyse_wav(GLIDE).unwrap();
    let voiced = vp.pitch.hz.iter().flatten().count();
    let positioned = vp.formants.vowel_space().len();

    let coverage = positioned as f32 / voiced as f32;
    assert!(
        coverage > 0.75,
        "only {:.0}% of voiced frames have both F1 and F2",
        coverage * 100.0
    );
}

#[test]
fn formants_move_while_the_pitch_stays_still() {
    // f0 barely moves here while the formants swing: they are not tracking the
    // source. (The synthetic tests show independence.)
    let vp = analyse_wav(GLIDE).unwrap();
    let f2_swing = median_over(&vp, &vp.formants.f2, EE.0, EE.1)
        - median_over(&vp, &vp.formants.f2, OO.0, OO.1);

    let mut pitches: Vec<f32> = vp.pitch.hz.iter().flatten().copied().collect();
    pitches.sort_by(f32::total_cmp);
    let pitch_swing = pitches[9 * pitches.len() / 10] - pitches[pitches.len() / 10];

    assert!(
        f2_swing > 1_000.0,
        "F2 moved only {f2_swing:.0} Hz across the glide"
    );
    assert!(
        pitch_swing < 20.0,
        "f0 moved {pitch_swing:.0} Hz — not a constant-pitch glide"
    );
}
