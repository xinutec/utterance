//! Noise characterisation against signals whose spectrum is known.
//!
//! Both measurements have textbook values on textbook signals, so these are not
//! a matter of judgement: white noise is flat and a sine is not, and a band of
//! noise has its centroid in the middle of the band.

mod common;

use utterance_analysis::resample::ANALYSIS_RATE;
use utterance_analysis::texture;

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

    let noise_flatness = steady_median(&noise.flatness);
    let tone_flatness = steady_median(&tone.flatness);
    assert!(
        noise_flatness > 0.4,
        "white noise measured only {noise_flatness:.3} flat"
    );
    assert!(
        tone_flatness < 0.05,
        "a sine measured {tone_flatness:.3} flat"
    );
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

#[test]
fn a_fricative_under_room_rumble_still_reads_as_noise() {
    // The failure that made the first version useless on real speech. Every
    // recording has low-frequency energy in it — room, proximity, the tail of
    // the last vowel — and measuring across the whole spectrum let that dominate
    // both numbers, so genuine consonants came back reading as tonal.
    let hiss = band(6_000.0, 3_000.0, 1.0);
    let rumble = band(80.0, 40.0, 1.0);
    let mixed: Vec<f32> = hiss
        .iter()
        .zip(&rumble)
        // Rumble far louder than the hiss, as it is in a real recording.
        .map(|(h, r)| h * 0.2 + r * 0.8)
        .collect();

    let t = texture::track(&mixed);
    let flat = steady_median(&t.flatness);
    let centre = steady_median(&t.centroid_hz);
    assert!(
        flat > 0.15,
        "a fricative buried under rumble measured only {flat:.3} flat"
    );
    assert!(
        centre > 2_000.0,
        "the centroid followed the rumble to {centre:.0} Hz"
    );
}

#[test]
fn nothing_below_the_band_reaches_either_measure() {
    // A pure low tone carries no information about consonants, and must not be
    // able to move a measurement that is about them.
    let t = texture::track(&common::sine(120.0, ANALYSIS_RATE, 1.0));
    assert!(
        steady_median(&t.centroid_hz) > texture::NOISE_BAND_LOW_HZ,
        "a 120 Hz tone moved a measurement that starts at 300 Hz"
    );
}

/// White noise shaped to a known slope, by a one-pole filter with a known one.
///
/// A single pole rolls off at 6 dB per octave above its corner, so cascading `n`
/// of them at a corner below the measured band gives −6·n dB/octave across it.
/// That makes the expected answer arithmetic rather than a number read off a
/// previous run of the code being tested.
fn sloped(poles: usize, secs: f32) -> Vec<f32> {
    let mut signal = white(secs);
    // Well below NOISE_BAND_LOW_HZ, so the whole fitted band is in the roll-off
    // rather than straddling the corner.
    let corner_hz = 50.0;
    let alpha = 1.0 - (-2.0 * std::f32::consts::PI * corner_hz / ANALYSIS_RATE as f32).exp();
    for _ in 0..poles {
        let mut y = 0.0f32;
        for sample in &mut signal {
            y += alpha * (*sample - y);
            *sample = y;
        }
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
fn tilt_reads_a_known_slope_in_decibels_per_octave() {
    // Not "steeper than the other one" — the actual number. A slope measurement
    // that only orders things correctly can be wrong by a factor and never say
    // so, and the unit is what a mapping would normalise against.
    for poles in [1usize, 2, 3] {
        let measured = steady_median(&texture::track(&sloped(poles, 1.0)).tilt_db_per_octave);
        let expected = -6.0 * poles as f32;
        assert!(
            (measured - expected).abs() < 1.5,
            "{poles} poles should fall at {expected:.0} dB/octave, measured {measured:.1}"
        );
    }
}

#[test]
fn tilt_measures_the_voice_and_not_the_resampler() {
    // The design decision, made falsifiable. Everything is analysed at 16 kHz and
    // a band-limited resampler's anti-aliasing filter collapses approaching 8 kHz.
    // A fit taken to Nyquist would measure that cliff on every frame of every
    // recording and report it as a property of the speaker — steeply, and
    // consistently enough to look like a real result.
    //
    // White noise is the signal that catches it: genuinely flat, so anything the
    // measurement finds is the machinery.
    let flat = steady_median(&texture::track(&white(1.0)).tilt_db_per_octave);
    assert!(
        flat.abs() < 2.0,
        "white noise reads as {flat:.1} dB/octave, so the fit is measuring the filter"
    );
}

#[test]
fn tilt_separates_two_vowels_the_centroid_agrees_about() {
    // Why this is a stream rather than a restatement of the brightness already
    // read. Two bands with the same centre have the same centroid by
    // construction, and a spectrum that falls away from it steeply is a different
    // sound from one that does not.
    let narrow = texture::track(&band(1_200.0, 200.0, 1.0));
    let wide = texture::track(&band(1_200.0, 2_000.0, 1.0));

    let centroids = (
        steady_median(&narrow.centroid_hz),
        steady_median(&wide.centroid_hz),
    );
    let tilts = (
        steady_median(&narrow.tilt_db_per_octave),
        steady_median(&wide.tilt_db_per_octave),
    );
    assert!(
        tilts.0 < tilts.1 - 3.0,
        "a narrow band and a wide one at the same place tilt alike: {:.1} and {:.1}",
        tilts.0,
        tilts.1
    );
    // Stated so the test fails loudly if the fixtures stop sharing a centroid and
    // the comparison quietly becomes about brightness after all.
    assert!(
        (centroids.0 - centroids.1).abs() < 600.0,
        "the two bands no longer share a centroid: {:.0} and {:.0} Hz",
        centroids.0,
        centroids.1
    );
}
