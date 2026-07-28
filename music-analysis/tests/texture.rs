//! Noise characterisation against signals whose spectrum is known.
//!
//! Both measurements have textbook values on textbook signals, so these are not
//! a matter of judgement: white noise is flat and a sine is not, and a band of
//! noise has its centroid in the middle of the band.

mod common;

use music_analysis::resample::ANALYSIS_RATE;
use music_analysis::texture;

/// Median of a series, ignoring the edge frames whose window is half padding.
fn steady_median(values: &[f32]) -> f32 {
    let margin = values.len() / 10;
    let mut middle: Vec<f32> = values[margin..values.len() - margin].to_vec();
    middle.sort_by(f32::total_cmp);
    middle[middle.len() / 2]
}

/// Deterministic white noise, so a test never depends on a seed nobody chose.
fn white(secs: f32) -> Vec<f32> {
    let n = (ANALYSIS_RATE as f32 * secs) as usize;
    let mut state: u32 = 0x1234_5678;
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

/// White noise through a one-pole resonator, giving a band around `centre_hz`.
fn band(centre_hz: f32, bandwidth_hz: f32, secs: f32) -> Vec<f32> {
    let mut signal = white(secs);
    let theta = 2.0 * std::f32::consts::PI * centre_hz / ANALYSIS_RATE as f32;
    let radius = (-std::f32::consts::PI * bandwidth_hz / ANALYSIS_RATE as f32).exp();
    let (a1, a2) = (2.0 * radius * theta.cos(), -radius * radius);

    let (mut y1, mut y2) = (0.0f32, 0.0f32);
    for sample in &mut signal {
        let y = *sample + a1 * y1 + a2 * y2;
        y2 = y1;
        y1 = y;
        *sample = y;
    }
    let peak = signal.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        for s in &mut signal {
            *s /= peak;
        }
    }
    signal
}

#[test]
fn white_noise_is_flat_and_a_tone_is_not() {
    // The definition of the measure: 1 for white noise, 0 for a pure tone.
    let noise = texture::track(&white(1.0));
    let tone = texture::track(&common::sine(440.0, ANALYSIS_RATE, 1.0));

    let noisy = steady_median(&noise.flatness);
    let tonal = steady_median(&tone.flatness);
    assert!(noisy > 0.4, "white noise measured only {noisy:.3} flat");
    assert!(tonal < 0.05, "a sine measured {tonal:.3} flat");
}

#[test]
fn a_vowel_is_far_more_tonal_than_a_fricative() {
    // The distinction the mapping actually needs: this is what separates the
    // material that carries pitch from the material that carries texture.
    let vowel = texture::track(&common::vowel(120.0, 1.0));
    let fricative = texture::track(&band(6_000.0, 2_000.0, 1.0));

    assert!(
        steady_median(&fricative.flatness) > steady_median(&vowel.flatness) * 3.0,
        "vowel {:.3} vs fricative {:.3}",
        steady_median(&vowel.flatness),
        steady_median(&fricative.flatness)
    );
}

#[test]
fn the_centroid_lands_inside_the_band_it_measures() {
    for centre in [1_000.0f32, 3_000.0, 6_000.0] {
        let measured = steady_median(&texture::track(&band(centre, 800.0, 1.0)).centroid_hz);
        assert!(
            (measured - centre).abs() < centre * 0.35,
            "a band at {centre} Hz measured a centroid of {measured:.0} Hz"
        );
    }
}

#[test]
fn a_hissed_s_reads_brighter_than_a_hushed_sh() {
    // The two most common fricatives in English sit about an octave apart, and
    // telling them apart is most of what makes consonants sound individual.
    let ess = steady_median(&texture::track(&band(7_000.0, 3_000.0, 1.0)).centroid_hz);
    let esh = steady_median(&texture::track(&band(3_500.0, 1_500.0, 1.0)).centroid_hz);
    assert!(ess > esh * 1.4, "s {ess:.0} Hz against sh {esh:.0} Hz");
}

#[test]
fn silence_reports_no_energy_anywhere_rather_than_a_confident_zero() {
    // A geometric mean collapses to zero if any bin does, so without a floor a
    // digitally silent frame reports itself as perfectly tonal — the most
    // confident possible answer about nothing at all.
    let quiet = texture::track(&vec![0.0; ANALYSIS_RATE as usize]);
    assert!(
        quiet.flatness.iter().all(|&f| f > 0.5),
        "silence read as tonal"
    );
    assert!(quiet.centroid_hz.iter().all(|&c| c.is_finite()));
}

#[test]
fn every_series_matches_the_frame_grid() {
    let t = texture::track(&common::vowel(140.0, 0.7));
    assert_eq!(t.centroid_hz.len(), t.flatness.len());
    assert!(!t.centroid_hz.is_empty());
}

#[test]
fn is_a_pure_function_of_its_input() {
    let signal = common::vowel(130.0, 0.5);
    let a = texture::track(&signal);
    let b = texture::track(&signal);
    assert_eq!(a.centroid_hz, b.centroid_hz);
    assert_eq!(a.flatness, b.flatness);
}
