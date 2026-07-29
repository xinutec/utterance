//! The lattice mapping, checked against the one thing it exists to do.
//!
//! `field` already turns a voice into a continuous texture, and these tests do
//! not repeat that. What is new here is that the harmony *stops moving* while
//! the mouth does not, and that when it moves it moves by a step rather than a
//! jump — which together are the whole argument for the mapping, since the
//! derived tuning has been measured as real and inaudible for want of a chord
//! that rings.

use utterance_analysis::partials::{Partial, Partials};
use utterance_analysis::speaker::{Brightness, Span, VowelSpace};
use utterance_analysis::texture::Texture;
use utterance_analysis::voiceprint::{Events, Formants, FrameGrid, Pitch, Source, Voiceprint};
use utterance_mapping::params::Params;
use utterance_mapping::tonnetz;
use utterance_mapping::voice::Voice;

fn space() -> VowelSpace {
    VowelSpace::new(300.0, 800.0, 900.0, 2400.0)
        .unwrap()
        .with_f3(Span::new(2000.0, 3200.0))
}

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

fn voice() -> Voice {
    let p = calibration();
    Voice::from_calibration(
        &p,
        &[&p],
        2.0,
        space(),
        Brightness::new(300.0, 3000.0),
        120.0,
    )
    .expect("a voice")
}

/// A take whose every per-frame series can be set independently.
fn take(frames: usize) -> Voiceprint {
    Voiceprint {
        schema_version: 7,
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
            aperiodicity: vec![0.05; frames],
        },
        formants: Formants {
            f1: vec![Some(550.0); frames],
            f2: vec![Some(1650.0); frames],
            f3: vec![None; frames],
        },
        rms_db: vec![-6.0; frames],
        events: Events {
            flux: vec![0.0; frames],
            onset_frames: Vec::new(),
            onset_times_s: Vec::new(),
        },
        partials: Partials {
            frames_used: 0,
            f0_hz: None,
            partials: Vec::new(),
        },
        texture: Texture {
            centroid_hz: vec![500.0; frames],
            flatness: vec![0.01; frames],
            tilt_db_per_octave: vec![-9.0; frames],
        },
    }
}

/// No drift, so a pitch that wanders cannot be mistaken for a chord that does.
fn still() -> Params {
    Params {
        drift: 0.0,
        ..Params::default()
    }
}

/// How many frames the chord is not the chord of the frame before it.
fn changes(field: &utterance_mapping::score::Field) -> usize {
    (1..field.frames())
        .filter(|&i| {
            (0..field.voice_count()).any(|v| {
                let (before, now) = (field.voices[v][i - 1], field.voices[v][i]);
                (now - before).abs() > 0.01
            })
        })
        .count()
}

/// The take's vowel swept slowly across the speaker's whole space.
fn swept(frames: usize) -> Voiceprint {
    let mut vp = take(frames);
    for i in 0..frames {
        let t = i as f32 / frames as f32;
        vp.formants.f1[i] = Some(320.0 + t * 460.0);
        vp.formants.f2[i] = Some(950.0 + t * 1400.0);
    }
    vp
}

#[test]
fn a_held_vowel_is_a_held_chord() {
    // The whole reason this mapping exists. In `field` the root slides
    // continuously, so no two consecutive frames are ever the same chord and
    // nothing rings long enough for its tuning to be audible.
    let vp = take(500);
    let f = tonnetz::compose_with(&vp, &voice(), still()).expect("a field");
    assert_eq!(
        changes(&f),
        0,
        "the chord moved while the mouth held perfectly still"
    );
}

#[test]
fn every_frame_of_the_take_still_reaches_the_field() {
    // Quantising the harmony must not quantise the time. Nothing is cut into
    // events and no frame is skipped; what holds still is the chord, not the
    // music.
    let vp = swept(500);
    let f = tonnetz::compose_with(&vp, &voice(), still()).unwrap();
    assert_eq!(f.frames(), 500);
    for v in &f.voices {
        assert_eq!(v.len(), 500);
    }
}

#[test]
fn a_moving_mouth_moves_the_harmony() {
    // The other half of the claim: holding is not being stuck.
    let vp = swept(600);
    let f = tonnetz::compose_with(&vp, &voice(), still()).unwrap();
    assert!(
        changes(&f) > 0,
        "a vowel swept across the whole space never changed the chord"
    );
}

#[test]
fn holding_makes_the_harmony_change_less_often() {
    // What the knob is for, stated as the thing it must do.
    let vp = swept(900);
    let free = tonnetz::compose_with(
        &vp,
        &voice(),
        Params {
            hold: 0.0,
            ..still()
        },
    )
    .unwrap();
    let held = tonnetz::compose_with(
        &vp,
        &voice(),
        Params {
            hold: 0.9,
            ..still()
        },
    )
    .unwrap();
    assert!(
        changes(&held) < changes(&free),
        "holding at 0.9 changed the chord {} times against {} when free",
        changes(&held),
        changes(&free)
    );
}

#[test]
fn a_chord_change_keeps_some_of_the_chord() {
    // Voice leading as geometry: adjacent triangles share two of three pitches,
    // so a change holds notes rather than replacing them. Nothing in the mapping
    // enforces this — it is what adjacency on the lattice is, and if it stops
    // being true the lattice has been laid out wrong.
    let vp = swept(900);
    let f = tonnetz::compose_with(&vp, &voice(), still()).unwrap();

    let moments: Vec<usize> = (1..f.frames())
        .filter(|&i| {
            (0..f.voice_count()).any(|v| (f.voices[v][i] - f.voices[v][i - 1]).abs() > 0.01)
        })
        .collect();
    assert!(!moments.is_empty(), "the fixture never changed chord");

    for i in moments {
        let before: Vec<i32> = (0..f.voice_count())
            .map(|v| f.voices[v][i - 1].round() as i32)
            .collect();
        let now: Vec<i32> = (0..f.voice_count())
            .map(|v| f.voices[v][i].round() as i32)
            .collect();
        let kept = now.iter().filter(|hz| before.contains(hz)).count();
        assert!(
            kept > 0,
            "a chord change at frame {i} replaced every voice: {before:?} then {now:?}"
        );
    }
}

#[test]
fn the_voices_are_stacked_upward_and_never_double() {
    // Two voices on one pitch is one voice twice as loud, which sounds thinner
    // than the voice count claims.
    let vp = take(300);
    let f = tonnetz::compose_with(&vp, &voice(), still()).unwrap();
    let pitches: Vec<f32> = (0..f.voice_count()).map(|v| f.voices[v][150]).collect();
    for pair in pitches.windows(2) {
        assert!(
            pair[1] > pair[0] * 1.02,
            "voices are not apart: {pitches:?}"
        );
    }
}

#[test]
fn spacing_opens_the_chord() {
    let vp = take(300);
    let close = tonnetz::compose_with(
        &vp,
        &voice(),
        Params {
            spacing: 1,
            ..still()
        },
    )
    .unwrap();
    let open = tonnetz::compose_with(
        &vp,
        &voice(),
        Params {
            spacing: 5,
            ..still()
        },
    )
    .unwrap();
    let top = close.voice_count() - 1;
    assert!(
        open.voices[top][150] > close.voices[top][150],
        "spacing did not open the chord: {} against {}",
        open.voices[top][150],
        close.voices[top][150]
    );
}

#[test]
fn the_mouth_shape_tips_the_chord_without_moving_it() {
    // F3 tips the weight between the top of the chord and the bottom. The pitches
    // are the triangle's and are untouched, which is the point: everything about
    // pitch here moves in lattice steps, so a stream reaching the harmony would
    // be silent for most of its travel and then jump.
    let mut rounded = take(300);
    for slot in rounded.formants.f3.iter_mut() {
        *slot = Some(2100.0);
    }
    let mut spread = take(300);
    for slot in spread.formants.f3.iter_mut() {
        *slot = Some(3100.0);
    }

    let voice = voice();
    let a = tonnetz::compose_with(&rounded, &voice, still()).unwrap();
    let b = tonnetz::compose_with(&spread, &voice, still()).unwrap();
    let top = a.voice_count() - 1;
    assert!(
        b.gains[top][150] > a.gains[top][150],
        "a spread mouth did not lift the top of the chord: {} against {}",
        b.gains[top][150],
        a.gains[top][150]
    );
    assert_eq!(a.voices, b.voices, "the mouth shape moved the harmony");
}

#[test]
fn voicing_at_zero_ignores_the_third_formant() {
    let mut rounded = take(300);
    for slot in rounded.formants.f3.iter_mut() {
        *slot = Some(2100.0);
    }
    let mut spread = take(300);
    for slot in spread.formants.f3.iter_mut() {
        *slot = Some(3100.0);
    }

    let off = Params {
        voicing: 0.0,
        ..still()
    };
    let voice = voice();
    assert_eq!(
        tonnetz::compose_with(&rounded, &voice, off).unwrap().gains,
        tonnetz::compose_with(&spread, &voice, off).unwrap().gains
    );
}

#[test]
fn is_a_pure_function_of_its_input() {
    let vp = swept(400);
    let v = voice();
    assert_eq!(
        tonnetz::compose_with(&vp, &v, still()),
        tonnetz::compose_with(&vp, &v, still())
    );
}

#[test]
fn refuses_a_scale_that_spans_no_plane() {
    // Two partials give a curve with one minimum, which is a line. Silence beats
    // a second vowel dimension that secretly reaches nothing.
    let thin = Partials {
        frames_used: 500,
        f0_hz: Some(120.0),
        partials: (1..=2)
            .map(|k| Partial {
                number: k,
                ratio: k as f32,
                amplitude: 1.0,
                presence: 1.0,
            })
            .collect(),
    };
    let voice =
        Voice::from_calibration(&thin, &[&thin], 2.0, space(), None, 120.0).expect("a voice");
    assert!(tonnetz::compose(&take(300), &voice).is_none());
}

#[test]
fn a_crowded_chord_stays_inside_the_range_a_person_hears() {
    // Twelve voices at the widest spacing would otherwise target more than eight
    // octaves above the tonic, and a voice count that puts half its voices past
    // hearing is a voice count that means nothing.
    let vp = take(300);
    let f = tonnetz::compose_with(
        &vp,
        &voice(),
        Params {
            voices: 12,
            spacing: 6,
            ..still()
        },
    )
    .unwrap();
    let highest = f.voices[f.voice_count() - 1][150];
    assert!(
        highest < 120.0 * 16.0,
        "the top voice sounded at {highest} Hz over a 120 Hz tonic"
    );
}
