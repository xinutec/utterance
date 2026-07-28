//! A voiceprint becomes a continuously sounding field.
//!
//! The mapping that stops computing a melody. Where `compose` reads onsets and
//! emits notes, this reads *every frame* and emits parameter streams, so nothing
//! is quantised into an event and nothing is discarded between events.
//!
//! **What each stream of the voice becomes:**
//!
//! - **f0** — the whole field transposes with it, heavily smoothed. His prosody
//!   becomes the piece's slow harmonic drift. This is the largest measurement in
//!   the voiceprint and until now nothing read it at all.
//! - **vowel frontness** — walks a root up and down the speaker's scale.
//! - **vowel openness** — how widely the voices spread around that root.
//! - **energy** — how loud the field is, and how many voices are audible in it.
//! - **vowel brightness** — the colour every voice is rendered in.
//! - **aperiodicity** — how much of the field is breath.
//!
//! Six streams, continuously, against the four the note mapping read at onsets
//! only.
//!
//! **The rule that keeps this from being resynthesis** is the same one as
//! everywhere else: the voice moves the law, not the notes. Nothing here plays
//! his pitch. His pitch bends a tuning system; his mouth chooses degrees within
//! it; the result is in his scale at his tonic, and is not the thing he said.

use music_analysis::voiceprint::Voiceprint;

use crate::compose::compose_noise;
use crate::score::{Field, Score};
use crate::voice::Voice;

/// How many voices sound at once.
///
/// Five. Enough that the result is a texture rather than a chord anyone counts,
/// few enough that each is separately audible — past about seven, added voices
/// thicken the sound without being heard as themselves.
pub const VOICES: usize = 5;

/// Scale degrees between one voice and the next.
///
/// Two, so the voices stack in thirds of whatever the speaker's scale turns out
/// to be rather than in seconds. In a scale with eight degrees that is a spread
/// of roughly two octaves across five voices.
const VOICE_SPACING: usize = 2;

/// Octaves the root moves across as the vowel travels front to back.
const ROOT_OCTAVES: f32 = 1.0;

/// How far the field transposes for the speaker's full pitch range, in octaves.
///
/// Deliberately small. His prosody spans an octave and a half over a phrase, and
/// following it directly would make the field lurch about — the point is drift
/// that a listener feels as the piece having a shape, not a melody traced in
/// parallel.
const DRIFT_OCTAVES: f32 = 0.25;

/// Frames the pitch drift is averaged over.
///
/// Two seconds. Long enough to cross several syllables, so what survives is the
/// phrase-level declination rather than the pitch of any particular word. This
/// is the slow timescale the note mapping had no way to express.
const DRIFT_FRAMES: usize = 200;

/// Frames the root position is averaged over.
///
/// A fifth of a second — about one syllable. Short enough that the harmony
/// follows the articulation, long enough that a single misfit formant frame
/// cannot jolt the whole field.
const ROOT_FRAMES: usize = 20;

/// Frames loudness is averaged over.
///
/// 80 ms. Fast enough to keep the speaker's dynamics, slow enough that the field
/// does not flutter at the syllable rate.
const LEVEL_FRAMES: usize = 8;

/// Quietest the field ever falls, relative to its loudest moment.
///
/// Never silent: a field that stops is a sequence of events again, and the
/// silences in speech are part of its shape rather than gaps in it. Low enough
/// to be heard as a rest.
const FLOOR: f32 = 0.02;

/// Build the continuously sounding field for a take.
///
/// Returns `None` when the take has no usable scale to place voices in.
pub fn compose(vp: &Voiceprint, voice: &Voice) -> Option<Field> {
    let degrees = &voice.tuning.degrees;
    // The octave duplicates the tonic, so it is not a separate choice.
    let choices = degrees.len().saturating_sub(1);
    if choices == 0 || vp.frame.count == 0 {
        return None;
    }

    let frames = vp.frame.count;

    // Every stream is smoothed at its own timescale, which is the point: the
    // field moves at several rates at once, as a voice does.
    let drift = smooth(&filled(&vp.pitch.hz), DRIFT_FRAMES);
    let (open_raw, front_raw) = vowel_track(vp, voice);
    let open = smooth(&open_raw, ROOT_FRAMES);
    let front = smooth(&front_raw, ROOT_FRAMES);
    let level = smooth(&linear_level(vp), LEVEL_FRAMES);

    let mut voices = vec![vec![0.0f32; frames]; VOICES];
    let mut gains = vec![vec![0.0f32; frames]; VOICES];
    let mut colour = vec![0.0f32; frames];
    let mut breath = vec![0.0f32; frames];

    for i in 0..frames {
        // His prosody, as a slow transposition of everything.
        //
        // Measured against the speaker's habitual pitch — the profile's tonic —
        // rather than against this take's own median. Against the take's median
        // an utterance spoken entirely higher produces identical music, because
        // its own average is always its own average: the thing that makes one
        // reading different from another would be normalised away. Against the
        // person, speaking above your usual pitch lifts the piece, which is the
        // utterance being the piece.
        let drift_octaves = (drift[i] / voice.tonic_hz).max(0.01).log2() * DRIFT_OCTAVES;
        let root_octaves = front[i].clamp(0.0, 1.0) * ROOT_OCTAVES;
        let base = voice.tonic_hz * 2f32.powf(drift_octaves + root_octaves);

        // An open vowel spreads the voices apart, a closed one gathers them.
        let spread = 1.0 + open[i].clamp(0.0, 1.0);

        for v in 0..VOICES {
            let step = ((v as f32 * VOICE_SPACING as f32 * spread) as usize).min(choices * 2);
            let degree = degrees[step % choices];
            let octave = (step / choices) as f32;
            voices[v][i] = base * 2f32.powf(octave) * degree.ratio;

            // Upper voices fade in as the speaker gets louder, so a quiet
            // passage is a thinner texture and not merely a softer one.
            let reach = (level[i] * VOICES as f32) - v as f32;
            gains[v][i] = (level[i] * reach.clamp(0.0, 1.0)).max(if v == 0 { FLOOR } else { 0.0 });
        }

        colour[i] = front[i].clamp(0.0, 1.0);
        breath[i] = breath_at(vp, i);
    }

    Some(Field {
        hop_s: vp.frame.hop_s,
        voices,
        gains,
        colour,
        breath,
    })
}

/// Vowel position per frame, carried across frames with no estimate.
///
/// A held vowel is still that vowel while a consonant interrupts it, so a gap in
/// the formant track is missing information rather than a jump to the middle of
/// the space. Carrying the last known position forward keeps the field moving
/// the way the mouth moved; interpolating to a default would make every
/// consonant a lurch toward the centre.
fn vowel_track(vp: &Voiceprint, voice: &Voice) -> (Vec<f32>, Vec<f32>) {
    let mut open = vec![0.5f32; vp.frame.count];
    let mut front = vec![0.5f32; vp.frame.count];
    let (mut last_open, mut last_front) = (0.5f32, 0.5f32);

    for i in 0..vp.frame.count {
        if let (Some(Some(f1)), Some(Some(f2))) = (vp.formants.f1.get(i), vp.formants.f2.get(i)) {
            let (o, f) = voice.space.normalise(*f1, *f2);
            last_open = o.clamp(0.0, 1.0);
            last_front = f.clamp(0.0, 1.0);
        }
        open[i] = last_open;
        front[i] = last_front;
    }
    (open, front)
}

/// Pitch per frame with unvoiced gaps carried across.
///
/// Same reasoning as the vowel track: an unvoiced frame is a frame with no
/// measurement, not a frame at zero hertz, and zero would drag the drift down
/// every time he pronounced a consonant.
fn filled(hz: &[Option<f32>]) -> Vec<f32> {
    let first = hz.iter().flatten().copied().next().unwrap_or(1.0);
    let mut last = first;
    hz.iter()
        .map(|h| {
            if let Some(v) = *h {
                last = v;
            }
            last
        })
        .collect()
}

/// The energy envelope as a linear 0..1, relative to the take's loudest moment.
fn linear_level(vp: &Voiceprint) -> Vec<f32> {
    let loudest = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    vp.rms_db
        .iter()
        .map(|db| 10f32.powf((db - loudest) / 20.0).clamp(0.0, 1.0))
        .collect()
}

/// Breath fraction at one frame, from how periodic the voice was there.
fn breath_at(vp: &Voiceprint, i: usize) -> f32 {
    let aperiodicity = vp.pitch.aperiodicity.get(i).copied().unwrap_or_default();
    (aperiodicity / 0.6).clamp(0.0, 1.0) * 0.3
}

/// Centred moving average over `window` frames.
///
/// Centred rather than trailing so the field moves *with* the voice rather than
/// lagging it by half the window, which at the drift timescale would be a second
/// of delay and audible as the music answering rather than accompanying.
fn smooth(values: &[f32], window: usize) -> Vec<f32> {
    if values.is_empty() || window <= 1 {
        return values.to_vec();
    }
    let half = window / 2;
    (0..values.len())
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(values.len());
            values[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
        })
        .collect()
}

/// A whole score for this take: the field, plus the speaker's consonants.
///
/// The consonants come from the same place the note mapping gets them. They are
/// events by nature — a consonant is a thing that happens at a moment — so they
/// stay a list whether the pitched material is a field or a stream of notes.
pub fn score(vp: &Voiceprint, voice: &Voice) -> Score {
    let loudest = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    Score {
        duration_s: vp.source.duration_s,
        palette: voice.palette.clone(),
        detune_cents: voice.detune_cents,
        noise: compose_noise(vp, loudest),
        field: compose(vp, voice),
        events: Vec::new(),
    }
}
