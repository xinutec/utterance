//! Formant tracking against signals whose formants are known by construction.

mod common;

use common::resonated_vowel;
use music_analysis::formant;
use music_analysis::frame::{self, SPECTRAL_WINDOW};

/// Median of a per-frame formant series, ignoring frames with no estimate.
fn median(values: &[Option<f32>]) -> Option<f32> {
    let mut present: Vec<f32> = values.iter().flatten().copied().collect();
    if present.is_empty() {
        return None;
    }
    present.sort_by(f32::total_cmp);
    Some(present[present.len() / 2])
}

/// Track a synthesised vowel and return the median F1, F2, F3.
fn track_vowel(f0: f32, formants: &[(f32, f32)]) -> (Option<f32>, Option<f32>, Option<f32>) {
    let samples = resonated_vowel(f0, formants, 0.5);
    let voiced = vec![true; frame::count(samples.len())];
    let frames = formant::track(&samples, &voiced);
    (
        median(&frames.iter().map(|f| f.f1).collect::<Vec<_>>()),
        median(&frames.iter().map(|f| f.f2).collect::<Vec<_>>()),
        median(&frames.iter().map(|f| f.f3).collect::<Vec<_>>()),
    )
}

/// Formant estimates within this many hertz of the truth are acceptable.
///
/// Generous by the standards of a clean synthetic signal, but the analysis
/// window holds only a few pitch periods and the harmonics of the source sit
/// every `f0` hertz — an estimate cannot resolve the resonance more finely than
/// the harmonics that sample it.
const TOLERANCE_HZ: f32 = 90.0;

#[test]
fn recovers_the_formants_of_a_back_vowel() {
    // Roughly the vowel in "father": F1 high, F2 low.
    let (f1, f2, f3) = track_vowel(120.0, &[(730.0, 80.0), (1090.0, 90.0), (2440.0, 120.0)]);
    assert!((f1.unwrap() - 730.0).abs() < TOLERANCE_HZ, "F1 = {f1:?}");
    assert!((f2.unwrap() - 1090.0).abs() < TOLERANCE_HZ, "F2 = {f2:?}");
    assert!((f3.unwrap() - 2440.0).abs() < TOLERANCE_HZ, "F3 = {f3:?}");
}

#[test]
fn recovers_the_formants_of_a_front_vowel() {
    // Roughly the vowel in "beet": F1 low, F2 very high — the opposite corner of
    // the vowel space from the test above, so the two together show the tracker
    // follows the vowel rather than reporting something fixed.
    let (f1, f2, _) = track_vowel(120.0, &[(270.0, 60.0), (2290.0, 110.0), (3010.0, 170.0)]);
    assert!((f1.unwrap() - 270.0).abs() < TOLERANCE_HZ, "F1 = {f1:?}");
    assert!((f2.unwrap() - 2290.0).abs() < TOLERANCE_HZ, "F2 = {f2:?}");
}

#[test]
fn recovers_the_formants_of_a_close_back_vowel() {
    // Roughly the vowel in "boot": both F1 and F2 low, the third corner.
    let (f1, f2, _) = track_vowel(120.0, &[(300.0, 60.0), (870.0, 90.0), (2240.0, 130.0)]);
    assert!((f1.unwrap() - 300.0).abs() < TOLERANCE_HZ, "F1 = {f1:?}");
    assert!((f2.unwrap() - 870.0).abs() < TOLERANCE_HZ, "F2 = {f2:?}");
}

#[test]
fn formants_do_not_move_with_pitch() {
    // The property that makes formants worth measuring separately from f0: the
    // same vowel at a different pitch is the same vowel. If these tracked f0 the
    // vowel-space mapping would be measuring the tune, not the mouth.
    let vowel = [(730.0, 80.0), (1090.0, 90.0), (2440.0, 120.0)];
    let (low_f1, low_f2, _) = track_vowel(95.0, &vowel);
    let (high_f1, high_f2, _) = track_vowel(200.0, &vowel);

    assert!(
        (low_f1.unwrap() - high_f1.unwrap()).abs() < TOLERANCE_HZ,
        "F1 moved with pitch"
    );
    assert!(
        (low_f2.unwrap() - high_f2.unwrap()).abs() < TOLERANCE_HZ,
        "F2 moved with pitch"
    );
}

#[test]
fn unvoiced_frames_report_nothing() {
    // Reporting formants where there is no periodic source would invent vowels
    // in the gaps between them.
    let samples = resonated_vowel(120.0, &[(730.0, 80.0), (1090.0, 90.0)], 0.5);
    let frames = formant::track(&samples, &vec![false; frame::count(samples.len())]);
    assert!(
        frames
            .iter()
            .all(|f| f.f1.is_none() && f.f2.is_none() && f.f3.is_none())
    );
}

#[test]
fn silence_produces_no_resonances() {
    assert!(formant::resonances(&vec![0.0f32; SPECTRAL_WINDOW]).is_empty());
}

#[test]
fn estimates_are_ordered_and_plausible() {
    let samples = resonated_vowel(
        120.0,
        &[(730.0, 80.0), (1090.0, 90.0), (2440.0, 120.0)],
        0.3,
    );
    let window = frame::windowed(&samples, 10, SPECTRAL_WINDOW);
    let found = formant::estimate(&window);

    let (f1, f2, f3) = (found.f1.unwrap(), found.f2.unwrap(), found.f3.unwrap());
    assert!(f1 < f2 && f2 < f3, "formants out of order: {f1} {f2} {f3}");
    assert!(f1 > 90.0, "F1 below the accepted floor: {f1}");
}

#[test]
fn tracking_is_deterministic() {
    // The root solver iterates. It must start from a fixed point and converge to
    // the same answer every run, or every voiceprint containing formants stops
    // being reproducible.
    let samples = resonated_vowel(130.0, &[(500.0, 70.0), (1500.0, 100.0)], 0.3);
    let voiced = vec![true; frame::count(samples.len())];
    assert_eq!(
        formant::track(&samples, &voiced),
        formant::track(&samples, &voiced)
    );
}
