//! A voiceprint becomes a continuously sounding field: every frame read, as
//! parameter streams, nothing cut into events.
//!
//! - **f0** — the whole field transposes with it, heavily smoothed: prosody as
//!   slow harmonic drift.
//! - **vowel frontness** walks a root through the speaker's scale; **openness**
//!   spreads the voices around it; **F3** opens or clusters the top.
//! - **spectral flux** stirs the texture: rhythm without notes.
//! - **energy** sets loudness and how many voices are audible.
//! - **spectral centroid** sets the colour, in the speaker's brightness range.
//! - **aperiodicity** sets how much is breath.
//!
//! **Each stream moves one thing**, so the count is how many things can move
//! independently. And the voice moves the law, not the notes: the speaker's
//! pitch bends a tuning, their mouth chooses degrees in it, and nothing plays
//! what they said.

use utterance_analysis::voiceprint::Voiceprint;

use crate::compose::field_score;
use crate::params::{self, Params};
use crate::score::{Field, Score};
use crate::streams::{self, DRIFT_FRAMES, LEVEL_FRAMES, ROOT_FRAMES};
use crate::voice::Voice;

/// Voices the field sounds with by default: a texture, each voice still audible.
pub const VOICES: usize = 5;

/// Quietest the field ever falls, relative to its loudest moment: never silent,
/// or it is a sequence of events again, but low enough to be a rest.
pub(crate) const FLOOR: f32 = 0.02;

/// Build the continuously sounding field for a take, or `None` with no scale to
/// place voices in.
pub fn compose(vp: &Voiceprint, voice: &Voice) -> Option<Field> {
    compose_with(vp, voice, Params::default())
}

/// Build the field with the knobs set explicitly.
pub fn compose_with(vp: &Voiceprint, voice: &Voice, params: Params) -> Option<Field> {
    let params = params.sane();
    let tuning = params::bind_toward_equal(&voice.tuning, params.bind);
    let degrees = &tuning.degrees;
    // The octave duplicates the tonic, so it is not a separate choice.
    let choices = degrees.len().saturating_sub(1);
    if choices == 0 || vp.frame.count == 0 {
        return None;
    }

    let frames = vp.frame.count;

    // Every stream at its own timescale: the field moves at several rates at once.
    let drift = streams::smooth(&streams::filled(&vp.pitch.hz), DRIFT_FRAMES);
    let (open_raw, front_raw) = streams::vowel(vp, voice);
    let open = streams::smooth(&open_raw, ROOT_FRAMES);
    let front = streams::smooth(&front_raw, ROOT_FRAMES);
    let level = streams::smooth(&streams::level(vp), LEVEL_FRAMES);
    let bright = streams::smooth(&streams::brightness(vp, voice), ROOT_FRAMES);
    let depth = streams::smooth(&streams::depth(vp, voice), ROOT_FRAMES);
    // Flux at the fastest timescale, or it becomes another loudness curve.
    let stir = streams::smooth(&vp.events.flux, LEVEL_FRAMES);

    let mut voices = vec![vec![0.0f32; frames]; params.voices];
    let mut gains = vec![vec![0.0f32; frames]; params.voices];
    let mut colour = vec![0.0f32; frames];
    let mut breath = vec![0.0f32; frames];

    for i in 0..frames {
        // The speaker's prosody as a slow transposition, against their habitual
        // pitch (the profile's tonic), not this take's median — against its own
        // median, a take spoken entirely higher would make identical music.
        let drift_octaves = (drift[i] / voice.tonic_hz).max(0.01).log2() * params.drift;
        let root_octaves = front[i].clamp(0.0, 1.0) * params.reach;
        let base = voice.tonic_hz * 2f32.powf(drift_octaves + root_octaves);

        // An open vowel spreads the voices apart, a closed one gathers them.
        let spread = 1.0 + open[i].clamp(0.0, 1.0);

        // F3 shapes the chord, centred on the speaker's range: high opens the
        // top, low clusters it.
        let skew = (depth[i].clamp(0.0, 1.0) - 0.5) * 2.0 * params.voicing;
        let top = (params.voices - 1).max(1) as f32;

        for v in 0..params.voices {
            // Scaled by height in the stack, so the root stays and the voicing
            // opens from the top.
            let lean = 1.0 + skew * (v as f32 / top);
            let raw = v as f32 * params.spacing as f32 * spread * lean;
            let step = (raw.max(0.0) as usize).min(choices * 2);
            let degree = degrees[step % choices];
            let octave = (step / choices) as f32;
            voices[v][i] = base * 2f32.powf(octave) * degree.ratio;

            gains[v][i] = voice_gain(level[i], stir[i], v, &params, 1.0);
        }

        colour[i] = bright[i].clamp(0.0, 1.0);
        breath[i] = streams::breath_at(vp, i);
    }

    Some(Field {
        hop_s: vp.frame.hop_s,
        voices,
        gains,
        colour,
        breath,
    })
}

/// A whole score for this take: the field, plus the speaker's consonants.
pub fn score(vp: &Voiceprint, voice: &Voice) -> Score {
    score_with(vp, voice, Params::default())
}

/// The same, with the knobs set explicitly.
pub fn score_with(vp: &Voiceprint, voice: &Voice, params: Params) -> Score {
    let params = params.sane();
    field_score(vp, voice, params, compose_with(vp, voice, params))
}

/// How loud voice `v` of a continuous field is at one frame. Upper voices fade in
/// as the speaker gets louder and lift with a moving mouth; `weight` is any
/// per-voice balance a mapping adds. Shared by both continuous mappings, so they
/// differ only in harmony.
pub(crate) fn voice_gain(level: f32, stir: f32, v: usize, params: &Params, weight: f32) -> f32 {
    let top = (params.voices - 1).max(1) as f32;
    let reach = (level * params.voices as f32) - v as f32;
    let stirred = 1.0 + params.articulation * stir.clamp(0.0, 1.0) * (v as f32 / top);
    (level * reach.clamp(0.0, 1.0) * stirred * weight).max(if v == 0 { FLOOR } else { 0.0 })
}
