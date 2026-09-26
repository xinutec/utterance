//! A voiceprint and a voice become a score of notes — the `notes` mapping.
//!
//! Every rule is a decision, meant to be replaced:
//!
//! - **when**: at a detected onset, lasting until the next
//! - **which degree**: from where the vowel sat front-to-back in the speaker's
//!   own vowel space; **which octave**: from open-to-closed
//! - **how loud**: from the energy envelope
//! - **what colour**: from where the vowel sat and moved during the note
//! - **how breathy**: from how periodic the voice was
//!
//! The weak link is the first: onsets mean *the spectrum changed*, not *a
//! syllable began*, so the rhythm is wrong until the stress hierarchy exists.
//! Colour follows the speaker's formant movement, but on derived pitches at
//! derived times: the mouth shapes the tone, it does not utter it.

use utterance_analysis::stats::median;
use utterance_analysis::voiceprint::Voiceprint;

use crate::params::Params;
use crate::score::{Event, Field, NoiseEvent, Score};
use crate::streams;
use crate::voice::Voice;

/// Octaves the register spans above the tonic: two, so it holds as one voice.
const REGISTER_OCTAVES: f32 = 2.0;

/// Longest a single note is held, in seconds: sustaining across a pause turns a
/// rest into a drone.
const MAX_NOTE_S: f32 = 1.2;

/// Shortest note worth sounding: below it attack and release overlap into a
/// click.
const MIN_NOTE_S: f32 = 0.08;

/// How far after an onset to look for a frame that knows its vowel. Onsets often
/// land on the consonant before it; 120 ms crosses a plosive burst without
/// reaching the next syllable.
const VOWEL_SEARCH_FRAMES: usize = 12;

/// Quietest note kept, relative to the loudest in the take: onsets fire in
/// near-silence too.
const SILENCE_FLOOR: f32 = 0.02;

/// Aperiodicity at which a note is fully breathy. Voicing is decided far below,
/// so a frame here is one the tracker barely believed.
const FULL_BREATH_APERIODICITY: f32 = 0.6;

/// Most of a note that may be noise, set by listening: past about a third,
/// breath stops being a quality of the tone and becomes hiss over it.
const MAX_BREATH: f32 = 0.3;

/// Turn a voiceprint into a score, in the world a [`Voice`] describes — the
/// speaker's calibration, not this take, so one sentence does not change key
/// with how much of the range it happened to use.
pub fn compose(vp: &Voiceprint, voice: &Voice) -> Score {
    compose_with(vp, voice, Params::default())
}

/// The same, with the knobs set explicitly.
pub fn compose_with(vp: &Voiceprint, voice: &Voice, params: Params) -> Score {
    let params = params.sane();
    let tuning = crate::params::bind_toward_equal(&voice.tuning, params.bind);
    // The octave duplicates the tonic, so it is not a separate choice.
    let degrees = &tuning.degrees;
    let choices = &degrees[..degrees.len().saturating_sub(1)];
    if choices.is_empty() {
        return empty(vp, voice);
    }

    let loudest = streams::loudest_db(vp);

    let onsets = &vp.events.onset_frames;
    let mut events = Vec::new();

    for (n, &frame) in onsets.iter().enumerate() {
        let Some((f1, f2)) = vowel_near(vp, frame) else {
            continue;
        };
        let amplitude = amplitude_at(vp, frame, loudest);
        if amplitude < SILENCE_FLOOR {
            continue;
        }

        let (open, front) = voice.space.normalise(f1, f2);
        let degree = choices[index_of(front, choices.len())];
        // Inverted: an open vowel is the big, low end of the register.
        let register = ((1.0 - open).clamp(0.0, 1.0) * REGISTER_OCTAVES).floor();

        let start_s = frame as f32 * vp.frame.hop_s;
        let next_s = onsets
            .get(n + 1)
            .map_or(vp.source.duration_s, |&f| f as f32 * vp.frame.hop_s);
        let duration_s = (next_s - start_s).clamp(MIN_NOTE_S, MAX_NOTE_S);

        // Colour tracks the vowel across the note, clamped to the last frame so
        // a note held to the end keeps its movement too.
        let end_frame = (frame + (duration_s / vp.frame.hop_s) as usize)
            .min(vp.formants.f1.len().saturating_sub(1));
        let colour_from = front.clamp(0.0, 1.0);
        let colour_to = vowel_near(vp, end_frame).map_or(colour_from, |(a, b)| {
            voice.space.normalise(a, b).1.clamp(0.0, 1.0)
        });

        events.push(Event {
            start_s,
            duration_s,
            hz: voice.tonic_hz * 2f32.powf(register) * degree.ratio,
            amplitude,
            colour_from,
            colour_to,
            breath: breath_at(vp, frame, end_frame),
        });
    }

    Score {
        duration_s: vp.source.duration_s,
        palette: voice.palette.clone(),
        detune_cents: voice.detune_cents,
        noise: compose_noise(vp, params.consonants),
        // Notes, not a field: the mappings are alternatives.
        field: None,
        events,
    }
}

/// A score whose pitched material is a continuous field, plus the speaker's
/// consonants — events whichever way the pitched material is made.
pub(crate) fn field_score(
    vp: &Voiceprint,
    voice: &Voice,
    params: Params,
    field: Option<Field>,
) -> Score {
    Score {
        duration_s: vp.source.duration_s,
        palette: voice.palette.clone(),
        detune_cents: voice.detune_cents,
        noise: compose_noise(vp, params.consonants),
        field,
        events: Vec::new(),
    }
}

/// A score with no notes in it, for a take nothing could be read from.
fn empty(vp: &Voiceprint, voice: &Voice) -> Score {
    Score {
        duration_s: vp.source.duration_s,
        palette: voice.palette.clone(),
        detune_cents: voice.detune_cents,
        events: Vec::new(),
        noise: Vec::new(),
        field: None,
    }
}

/// How much of a note should be breath: the median aperiodicity over the voiced
/// frames it spans. A single onset frame measures the transition, several times
/// as aperiodic, and unvoiced frames are consonants the noise stream already
/// sounds.
fn breath_at(vp: &Voiceprint, from: usize, to: usize) -> f32 {
    let voiced: Vec<f32> = (from..to.min(vp.pitch.hz.len()))
        .filter(|&i| vp.pitch.hz[i].is_some())
        .map(|i| vp.pitch.aperiodicity[i])
        .collect();
    median(&voiced).map_or(0.0, |m| {
        (m / FULL_BREATH_APERIODICITY).clamp(0.0, 1.0) * MAX_BREATH
    })
}

/// Which degree a normalised position picks. Clamping happens here, where a
/// scale's ends make it necessary, not in the measurement.
///
/// Frontness also picks the colour, so those two move together where the voice
/// offered them separately — a known cost of this mapping.
fn index_of(position: f32, count: usize) -> usize {
    let scaled = position * (count - 1) as f32;
    (scaled.round().max(0.0) as usize).min(count - 1)
}

/// F1 and F2 at or shortly after `frame`, if any frame there knows them.
fn vowel_near(vp: &Voiceprint, frame: usize) -> Option<(f32, f32)> {
    (frame..(frame + VOWEL_SEARCH_FRAMES).min(vp.formants.f1.len()))
        .find_map(|i| Some((vp.formants.f1[i]?, vp.formants.f2[i]?)))
}

/// Loudness at a frame, relative to the loudest moment in the take.
fn amplitude_at(vp: &Voiceprint, frame: usize, loudest_db: f32) -> f32 {
    let db = vp.rms_db.get(frame).copied().unwrap_or(f32::NEG_INFINITY);
    streams::relative_amplitude(db, loudest_db)
}

/// Flatness above which a frame counts as noise rather than tone.
///
/// Set on real speech with [`NOISE_FLOOR`], counting selected runs centred below
/// 1.5 kHz (room, not voice): **strict about shape, lenient about level**.
/// Quietness is a property consonants genuinely have, so screening hard on level
/// throws away the real ones first.
const NOISE_FLATNESS: f32 = 0.20;

/// Shortest run of noise worth sounding (30 ms): discards the stray frames at
/// the edge of every voiced stretch, which would sound as clicks.
const MIN_NOISE_FRAMES: usize = 3;

/// Longest a run of noise is sounded, in seconds: silence is as flat as a
/// fricative, and no consonant lasts a second.
const MAX_NOISE_S: f32 = 0.5;

/// Quietest noise run kept, relative to the loudest moment (about 36 dB down):
/// a fricative is far quieter than a stressed vowel. See [`NOISE_FLATNESS`].
const NOISE_FLOOR: f32 = 0.015;

/// Bandwidth given to a fully flat frame, in Hz: flatness maps a peaked spectrum
/// to a narrow band that whistles and a flat one to air.
const MAX_NOISE_BANDWIDTH_HZ: f32 = 4_000.0;

/// The narrowest band, so a highly tonal frame still sounds like something.
const MIN_NOISE_BANDWIDTH_HZ: f32 = 250.0;

/// Turn the unvoiced stretches of a take into noise events, one per run of
/// noise-like frames, keeping the speaker's own consonant timing — the fastest
/// structural layer in speech.
pub fn compose_noise(vp: &Voiceprint, level: f32) -> Vec<NoiseEvent> {
    if level <= 0.0 {
        return Vec::new();
    }
    let loudest_db = streams::loudest_db(vp);
    let flatness = &vp.texture.flatness;
    let centroid = &vp.texture.centroid_hz;
    let voiced = &vp.pitch.hz;

    let mut out = Vec::new();
    let mut run: Option<usize> = None;

    for i in 0..=flatness.len() {
        let noisy = i < flatness.len()
            && voiced.get(i).copied().flatten().is_none()
            && flatness[i] >= NOISE_FLATNESS;

        match (noisy, run) {
            (true, None) => run = Some(i),
            (false, Some(start)) => {
                if let Some(mut event) = noise_run(vp, start, i, centroid, flatness, loudest_db) {
                    event.amplitude *= level;
                    out.push(event);
                }
                run = None;
            }
            _ => {}
        }
    }
    out
}

/// One run of noise frames as an event, if it is worth sounding.
fn noise_run(
    vp: &Voiceprint,
    start: usize,
    end: usize,
    centroid: &[f32],
    flatness: &[f32],
    loudest_db: f32,
) -> Option<NoiseEvent> {
    if end - start < MIN_NOISE_FRAMES {
        return None;
    }

    // Loudest frame, not the mean: a plosive is a burst followed by nothing.
    let amplitude = (start..end)
        .map(|i| amplitude_at(vp, i, loudest_db))
        .fold(0.0f32, f32::max);
    if amplitude < NOISE_FLOOR {
        return None;
    }

    let span = (end - start) as f32;
    let mean = |series: &[f32]| series[start..end].iter().sum::<f32>() / span;
    let flat = mean(flatness).clamp(0.0, 1.0);

    Some(NoiseEvent {
        start_s: start as f32 * vp.frame.hop_s,
        duration_s: (span * vp.frame.hop_s).min(MAX_NOISE_S),
        centre_hz: mean(centroid),
        bandwidth_hz: MIN_NOISE_BANDWIDTH_HZ
            + (MAX_NOISE_BANDWIDTH_HZ - MIN_NOISE_BANDWIDTH_HZ) * flat,
        amplitude,
    })
}
