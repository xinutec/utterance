//! The continuous field, checked against the rules it says it follows.
//!
//! What these can establish is that every stream reaches the output and moves
//! the right thing. That the result is musical, they cannot say.

use music_analysis::partials::{Partial, Partials};
use music_analysis::speaker::VowelSpace;
use music_analysis::texture::Texture;
use music_analysis::voiceprint::{Events, Formants, FrameGrid, Pitch, Source, Voiceprint};
use music_mapping::field::{self, VOICES};
use music_mapping::voice::Voice;

fn space() -> VowelSpace {
    VowelSpace::new(300.0, 800.0, 900.0, 2400.0).unwrap()
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
    Voice::from_calibration(&p, &[&p], 2.0, space(), 120.0).expect("a voice")
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
        },
    }
}

#[test]
fn every_frame_of_the_take_reaches_the_field() {
    // The whole point: nothing is quantised into events, so the field is exactly
    // as long as the measurement is.
    let vp = take(500);
    let f = field::compose(&vp, &voice()).expect("a field");
    assert_eq!(f.frames(), 500);
    assert_eq!(f.voice_count(), VOICES);
    for v in &f.voices {
        assert_eq!(v.len(), 500);
    }
}

#[test]
fn several_voices_sound_at_once() {
    // Where the note mapping had one pitch at a time, this must have a chord.
    let vp = take(300);
    let f = field::compose(&vp, &voice()).unwrap();
    let audible = (0..VOICES).filter(|&v| f.gains[v][150] > 0.0).count();
    assert!(
        audible >= 3,
        "only {audible} voices audible in a loud passage"
    );

    let pitches: Vec<f32> = (0..audible).map(|v| f.voices[v][150]).collect();
    for pair in pitches.windows(2) {
        assert!(
            pair[1] > pair[0],
            "voices are not stacked upward: {pitches:?}"
        );
    }
}

#[test]
fn the_field_never_stops() {
    // A field that falls silent is a sequence of events again, and the silences
    // in speech are part of its shape rather than gaps in it.
    let mut vp = take(300);
    for slot in vp.rms_db.iter_mut().take(200).skip(100) {
        *slot = -90.0;
    }
    let f = field::compose(&vp, &voice()).unwrap();
    assert!(
        f.gains[0][150] > 0.0,
        "the field went silent during a pause"
    );
}

#[test]
fn a_quiet_passage_is_thinner_than_a_loud_one() {
    // Loudness has to change the texture, not only the level — otherwise the
    // dynamics are a volume knob rather than something musical.
    let mut vp = take(400);
    for slot in vp.rms_db.iter_mut().take(300).skip(200) {
        *slot = -30.0;
    }
    let f = field::compose(&vp, &voice()).unwrap();
    let audible_at = |i: usize| (0..VOICES).filter(|&v| f.gains[v][i] > 0.01).count();
    assert!(
        audible_at(250) < audible_at(100),
        "quiet passage had {} voices against {} in the loud one",
        audible_at(250),
        audible_at(100)
    );
}

#[test]
fn his_prosody_transposes_the_whole_field() {
    // f0 is the largest measurement in the voiceprint and until this mapping
    // nothing read it at all.
    let mut low = take(600);
    let mut high = take(600);
    for slot in high.pitch.hz.iter_mut() {
        *slot = Some(200.0);
    }
    for slot in low.pitch.hz.iter_mut() {
        *slot = Some(100.0);
    }

    let v = voice();
    let a = field::compose(&low, &v).unwrap().voices[0][300];
    let b = field::compose(&high, &v).unwrap().voices[0][300];
    assert!(
        b > a * 1.05,
        "a higher voice did not lift the field: {a} then {b}"
    );
}

#[test]
fn the_vowel_walks_the_root_through_the_scale() {
    let mut back = take(600);
    let mut fronted = take(600);
    for slot in fronted.formants.f2.iter_mut() {
        *slot = Some(2350.0);
    }
    for slot in back.formants.f2.iter_mut() {
        *slot = Some(950.0);
    }

    let v = voice();
    let a = field::compose(&back, &v).unwrap().voices[0][300];
    let b = field::compose(&fronted, &v).unwrap().voices[0][300];
    assert!(b > a * 1.2, "frontness did not move the root: {a} then {b}");
}

#[test]
fn a_vowel_that_drops_out_holds_its_position() {
    // A held vowel is still that vowel while a consonant interrupts it. Falling
    // back to the middle of the space would make every consonant a lurch.
    let mut vp = take(400);
    for slot in vp.formants.f2.iter_mut() {
        *slot = Some(2350.0);
    }
    for i in 200..240 {
        vp.formants.f1[i] = None;
        vp.formants.f2[i] = None;
    }
    let f = field::compose(&vp, &voice()).unwrap();
    let ratio = f.voices[0][220] / f.voices[0][150];
    assert!(
        (ratio - 1.0).abs() < 0.05,
        "the field moved by {ratio:.2}x across a gap in the formant track"
    );
}

#[test]
fn refuses_a_take_it_cannot_place_voices_in() {
    let vp = take(0);
    assert!(field::compose(&vp, &voice()).is_none());
}

#[test]
fn is_a_pure_function_of_its_input() {
    let vp = take(300);
    let v = voice();
    assert_eq!(field::compose(&vp, &v), field::compose(&vp, &v));
}
