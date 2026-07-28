//! Composition, checked against the rules it says it follows.
//!
//! None of these assert that the result is musical — nothing can. They assert
//! that the mapping does what its own documentation claims: a note per onset, a
//! degree chosen by frontness, a register chosen by openness, dynamics carried
//! from the energy envelope. If a rule here is changed on purpose, the matching
//! test should change with it rather than be deleted.

use utterance_analysis::partials::{Partial, Partials};
use utterance_analysis::speaker::VowelSpace;
use utterance_analysis::texture::Texture;
use utterance_analysis::voiceprint::{Events, Formants, FrameGrid, Pitch, Source, Voiceprint};
use utterance_mapping::compose::compose;
use utterance_mapping::score::centroid;
use utterance_mapping::voice::Voice;

/// A speaker whose vowel space is a convenient unit square in Hz.
fn space() -> VowelSpace {
    VowelSpace::new(300.0, 800.0, 900.0, 2400.0).unwrap()
}

/// A harmonic series rich enough to derive a scale with several degrees from.
fn calibration() -> Partials {
    Partials {
        frames_used: 500,
        f0_hz: Some(120.0),
        partials: (1..=16)
            .map(|k| Partial {
                number: k,
                ratio: k as f32,
                amplitude: 0.9f32.powi(k as i32),
                presence: 1.0,
            })
            .collect(),
    }
}

/// A second, brighter spectrum, so the palette has an axis to travel along.
fn brighter() -> Partials {
    Partials {
        frames_used: 500,
        f0_hz: Some(120.0),
        partials: (1..=16)
            .map(|k| Partial {
                number: k,
                ratio: k as f32,
                // Rising toward the top rather than falling: a genuinely
                // different colour from `calibration`, not a louder copy.
                amplitude: 0.2 + 0.05 * k as f32,
                presence: 1.0,
            })
            .collect(),
    }
}

fn voice() -> Voice {
    let dark = calibration();
    let light = brighter();
    Voice::from_calibration(&dark, &[&dark, &light], 4.0, space(), None, 120.0)
        .expect("a rich spectrum gives a voice")
}

/// A take with onsets at the given frames, each carrying the vowel beside it.
///
/// `vowels` gives `(f1, f2)` per onset; every frame from that onset onward holds
/// it until the next, which is what a sustained vowel actually looks like.
fn take(onsets: &[usize], vowels: &[(f32, f32)], frames: usize, loud: bool) -> Voiceprint {
    let mut f1 = vec![None; frames];
    let mut f2 = vec![None; frames];
    for (n, &start) in onsets.iter().enumerate() {
        let end = onsets.get(n + 1).copied().unwrap_or(frames).min(frames);
        for i in start..end {
            f1[i] = Some(vowels[n].0);
            f2[i] = Some(vowels[n].1);
        }
    }

    Voiceprint {
        schema_version: 5,
        source: Source {
            sample_rate_hz: 16_000,
            channels: 1,
            duration_s: frames as f32 / 100.0,
            peak: 0.5,
            clipped_fraction: 0.0,
        },
        frame: FrameGrid {
            analysis_rate_hz: 16_000,
            hop_s: 0.01,
            count: frames,
        },
        pitch: Pitch {
            hz: vec![Some(120.0); frames],
            aperiodicity: vec![0.1; frames],
        },
        formants: Formants {
            f3: vec![None; frames],
            f1,
            f2,
        },
        rms_db: vec![if loud { -6.0 } else { -60.0 }; frames],
        events: Events {
            flux: vec![0.0; frames],
            onset_frames: onsets.to_vec(),
            onset_times_s: onsets.iter().map(|&i| i as f32 * 0.01).collect(),
        },
        partials: Partials {
            frames_used: 0,
            f0_hz: None,
            partials: Vec::new(),
        },
        // Tonal and dark by default, so a take says nothing about consonants
        // unless a test deliberately puts some in.
        texture: Texture {
            centroid_hz: vec![500.0; frames],
            flatness: vec![0.01; frames],
        },
    }
}

/// Mid-openness vowels at the back, the middle and the front of the space.
const BACK: (f32, f32) = (550.0, 950.0);
const MIDDLE: (f32, f32) = (550.0, 1650.0);
const FRONT: (f32, f32) = (550.0, 2350.0);

#[test]
fn sounds_a_note_at_every_onset_that_has_a_vowel() {
    let vp = take(&[0, 50, 100], &[BACK, MIDDLE, FRONT], 200, true);
    let score = compose(&vp, &voice());
    assert_eq!(score.events.len(), 3);
    for (event, &frame) in score.events.iter().zip(&vp.events.onset_frames) {
        assert!((event.start_s - frame as f32 * 0.01).abs() < 1e-6);
    }
}

#[test]
fn a_fronter_vowel_takes_a_higher_degree() {
    // The rule the mapping documents: frontness picks the scale degree.
    let vp = take(&[0, 50, 100], &[BACK, MIDDLE, FRONT], 200, true);
    let score = compose(&vp, &voice());
    assert!(
        score.events[0].hz < score.events[1].hz && score.events[1].hz < score.events[2].hz,
        "front-to-back did not order the pitches: {:?}",
        score.events.iter().map(|e| e.hz).collect::<Vec<_>>()
    );
}

#[test]
fn an_opener_vowel_drops_a_register() {
    // Openness picks the octave, inverted: an open mouth is the low, big end.
    let closed = take(&[0], &[(320.0, 1650.0)], 100, true);
    let open = take(&[0], &[(780.0, 1650.0)], 100, true);
    let v = voice();

    let high = compose(&closed, &v).events[0].hz;
    let low = compose(&open, &v).events[0].hz;
    assert!(
        high > low * 1.9,
        "expected roughly two octaves between the extremes, got {low:.0} and {high:.0} Hz"
    );
}

#[test]
fn drops_onsets_that_fired_in_the_quiet_parts_of_a_take() {
    // Onset detection fires in the gaps between phrases too, and a note there is
    // an artefact of the detector rather than something the speaker did.
    let mut vp = take(&[0, 50], &[MIDDLE, MIDDLE], 100, true);
    for slot in vp.rms_db.iter_mut().skip(50) {
        *slot = -60.0;
    }
    let score = compose(&vp, &voice());
    assert_eq!(
        score.events.len(),
        1,
        "the near-silent onset should not sound"
    );
    assert!((score.events[0].start_s).abs() < 1e-6);
}

#[test]
fn judges_loudness_against_the_take_rather_than_full_scale() {
    // Deliberate: the shape of the envelope is the measurement, not the level it
    // happened to be recorded at. A whole take recorded quietly is a quiet
    // performance of the same music, not a silent one — so it still sounds, and
    // it sounds identical to a loud recording of the same gestures.
    let loud = take(&[0, 50], &[BACK, FRONT], 100, true);
    let quiet = take(&[0, 50], &[BACK, FRONT], 100, false);
    let v = voice();
    assert_eq!(compose(&loud, &v).events, compose(&quiet, &v).events);
    assert_eq!(compose(&quiet, &v).events.len(), 2);
}

#[test]
fn drops_onsets_with_no_vowel_to_read() {
    // An onset in a stretch with no formant estimate — an unvoiced consonant, or
    // noise — has no position in the vowel space and so no degree.
    let mut vp = take(&[0, 50], &[MIDDLE, MIDDLE], 100, true);
    for slot in vp.formants.f1.iter_mut().skip(50) {
        *slot = None;
    }
    assert_eq!(compose(&vp, &voice()).events.len(), 1);
}

#[test]
fn holds_a_note_until_the_next_onset() {
    let vp = take(&[0, 30], &[MIDDLE, MIDDLE], 100, true);
    let score = compose(&vp, &voice());
    assert!((score.events[0].duration_s - 0.30).abs() < 1e-5);
}

#[test]
fn never_sustains_across_a_long_silence() {
    // A gap between onsets can be several seconds. Sustaining across one turns a
    // rest into a drone.
    let vp = take(&[0, 900], &[MIDDLE, MIDDLE], 1000, true);
    let score = compose(&vp, &voice());
    assert!(
        score.events[0].duration_s < 2.0,
        "held a note for {:.1}s across a nine-second gap",
        score.events[0].duration_s
    );
}

#[test]
fn carries_the_dynamics_of_the_take() {
    let mut vp = take(&[0, 50], &[MIDDLE, MIDDLE], 100, true);
    for slot in vp.rms_db.iter_mut().skip(50) {
        *slot = -18.0;
    }
    let score = compose(&vp, &voice());
    // 12 dB down is a quarter of the amplitude.
    let ratio = score.events[1].amplitude / score.events[0].amplitude;
    assert!(
        (ratio - 0.25).abs() < 0.02,
        "a 12 dB drop rendered as an amplitude ratio of {ratio:.3}"
    );
}

#[test]
fn carries_the_speakers_own_palette_into_the_score() {
    // The score is what reaches the synthesiser, and a tuning derived from one
    // spectrum is only consonant for tones that have it.
    let vp = take(&[0], &[MIDDLE], 100, true);
    let v = voice();
    let score = compose(&vp, &v);
    assert_eq!(score.palette, v.palette);
    assert_eq!(score.detune_cents, v.detune_cents);
    assert_eq!(
        v.palette.len(),
        2,
        "both calibration spectra should survive"
    );
}

#[test]
fn orders_the_palette_dark_to_bright() {
    // `colour` only means anything if the axis is ordered, and brightness is the
    // one a listener can follow.
    let v = voice();
    let centroids: Vec<f32> = v.palette.iter().map(|s| centroid(s)).collect();
    assert!(
        centroids[0] < centroids[1],
        "palette is not ordered by brightness: {centroids:?}"
    );
}

#[test]
fn a_vowel_that_moves_gives_a_note_whose_colour_moves() {
    // The reason colour is two numbers. A syllable whose mouth travels should
    // produce a tone that travels with it.
    let mut vp = take(&[0], &[BACK], 100, true);
    for i in 40..100 {
        vp.formants.f1[i] = Some(FRONT.0);
        vp.formants.f2[i] = Some(FRONT.1);
    }
    let event = &compose(&vp, &voice()).events[0];
    assert!(
        event.colour_to > event.colour_from + 0.3,
        "colour barely moved: {} to {}",
        event.colour_from,
        event.colour_to
    );
}

#[test]
fn a_steady_vowel_gives_a_note_whose_colour_holds() {
    let vp = take(&[0], &[MIDDLE], 100, true);
    let event = &compose(&vp, &voice()).events[0];
    assert!((event.colour_to - event.colour_from).abs() < 1e-6);
}

#[test]
fn a_less_periodic_voice_gives_a_breathier_note() {
    // Aperiodicity was measured from the first commit and read by nothing until
    // now — one of the streams the mapping was throwing away.
    let mut clean = take(&[0], &[MIDDLE], 100, true);
    for slot in clean.pitch.aperiodicity.iter_mut() {
        *slot = 0.02;
    }
    let mut breathy = take(&[0], &[MIDDLE], 100, true);
    for slot in breathy.pitch.aperiodicity.iter_mut() {
        *slot = 0.5;
    }

    let v = voice();
    let quiet_breath = compose(&clean, &v).events[0].breath;
    let much_breath = compose(&breathy, &v).events[0].breath;
    assert!(
        much_breath > quiet_breath + 0.2,
        "breath did not follow aperiodicity: {quiet_breath} vs {much_breath}"
    );
    assert!(much_breath < 1.0, "a note should never be entirely noise");
}

#[test]
fn puts_every_note_inside_the_speakers_range() {
    // Whatever the vowels do, the result stays within the tonic and the register
    // span above it — an articulation past the speaker's usual reach must not
    // send a note somewhere unplayable.
    let wild = take(
        &[0, 20, 40, 60],
        &[
            (100.0, 200.0),
            (2000.0, 5000.0),
            (-50.0, 900.0),
            (780.0, 2350.0),
        ],
        100,
        true,
    );
    let v = voice();
    for event in compose(&wild, &v).events {
        assert!(
            event.hz >= v.tonic_hz * 0.99 && event.hz <= v.tonic_hz * 4.01,
            "note at {:.0} Hz is outside the two octaves above a {:.0} Hz tonic",
            event.hz,
            v.tonic_hz
        );
    }
}

#[test]
fn is_a_pure_function_of_its_input() {
    let vp = take(&[0, 50, 100], &[BACK, MIDDLE, FRONT], 200, true);
    let v = voice();
    assert_eq!(compose(&vp, &v), compose(&vp, &v));
}

/// Mark frames `from..to` as unvoiced noise with the given spectral shape.
fn make_noisy(vp: &mut Voiceprint, from: usize, to: usize, centroid_hz: f32, flatness: f32) {
    for i in from..to {
        vp.pitch.hz[i] = None;
        vp.formants.f1[i] = None;
        vp.formants.f2[i] = None;
        vp.texture.centroid_hz[i] = centroid_hz;
        vp.texture.flatness[i] = flatness;
    }
}

#[test]
fn a_consonant_becomes_a_noise_event() {
    // The material every earlier version discarded: nearly three quarters of
    // ordinary speech carries no fundamental.
    let mut vp = take(&[0], &[MIDDLE], 100, true);
    make_noisy(&mut vp, 40, 60, 7000.0, 0.8);

    let score = compose(&vp, &voice());
    assert_eq!(score.noise.len(), 1, "the consonant did not sound");
    let n = &score.noise[0];
    assert!((n.start_s - 0.40).abs() < 1e-5);
    assert!((n.duration_s - 0.20).abs() < 1e-5);
    assert!((n.centre_hz - 7000.0).abs() < 1.0, "centre {}", n.centre_hz);
}

#[test]
fn a_flatter_consonant_gets_a_wider_band() {
    // Flatness is what separates air from a whistle, and it is the speaker's
    // own measurement that decides which.
    let mut airy = take(&[0], &[MIDDLE], 100, true);
    make_noisy(&mut airy, 40, 60, 5000.0, 0.95);
    let mut focused = take(&[0], &[MIDDLE], 100, true);
    make_noisy(&mut focused, 40, 60, 5000.0, 0.2);

    let v = voice();
    let wide = compose(&airy, &v).noise[0].bandwidth_hz;
    let narrow = compose(&focused, &v).noise[0].bandwidth_hz;
    assert!(wide > narrow * 2.0, "wide {wide} against narrow {narrow}");
}

#[test]
fn a_vowel_never_becomes_a_consonant() {
    // Voiced frames are tonal and must stay out of the noise stream entirely,
    // or the pitched material would be doubled as hiss.
    let vp = take(&[0, 50], &[BACK, FRONT], 100, true);
    assert!(compose(&vp, &voice()).noise.is_empty());
}

#[test]
fn a_single_stray_frame_is_not_a_consonant() {
    // One noise-like frame appears at the edge of almost every voiced stretch.
    // Sounding them would pepper the render with clicks nobody made.
    let mut vp = take(&[0], &[MIDDLE], 100, true);
    make_noisy(&mut vp, 50, 51, 6000.0, 0.9);
    assert!(compose(&vp, &voice()).noise.is_empty());
}

#[test]
fn a_silent_gap_is_not_a_consonant() {
    // Room tone between phrases measures as flat as a fricative does — there is
    // no energy in it, so there is no shape to it either.
    let mut vp = take(&[0], &[MIDDLE], 200, true);
    make_noisy(&mut vp, 60, 160, 4000.0, 0.9);
    for slot in vp.rms_db.iter_mut().take(160).skip(60) {
        *slot = -70.0;
    }
    assert!(
        compose(&vp, &voice()).noise.is_empty(),
        "silence was sounded as a consonant"
    );
}

#[test]
fn consonants_keep_the_speakers_own_timing() {
    // The fastest structural layer in speech, and the one the note stream
    // cannot carry: several consonants a second, where notes arrive at one or
    // two.
    let mut vp = take(&[0], &[MIDDLE], 300, true);
    for k in 0..5 {
        make_noisy(&mut vp, 40 + k * 40, 40 + k * 40 + 10, 6000.0, 0.8);
    }
    let noise = compose(&vp, &voice()).noise;
    assert_eq!(noise.len(), 5);
    for pair in noise.windows(2) {
        assert!(
            pair[1].start_s > pair[0].start_s,
            "noise events out of order"
        );
    }
}
