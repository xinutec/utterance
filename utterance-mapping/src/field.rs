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
//! - **F3** — opens or clusters the chord above that spread. The dimension of
//!   articulation the vowel chart cannot see: rounding and retroflexion move it
//!   while F1 and F2 hold still.
//! - **spectral flux** — stirs the texture where the mouth is moving fastest.
//!   Rhythm without cutting anything into notes.
//! - **energy** — how loud the field is, and how many voices are audible in it.
//! - **spectral centroid** — the colour every voice is rendered in, placed in
//!   the speaker's own brightness range.
//! - **aperiodicity** — how much of the field is breath.
//!
//! Eight streams, continuously, against the four the note mapping read at
//! onsets only.
//!
//! **Colour was frontness until it was measured.** The colour stream was set
//! from the same normalised F2 that walks the root, so the timbre and the
//! harmony moved as one thing: every chord change was also the only colour
//! change, and the field had five voices doing four things. Brightness is
//! measured independently — the same vowel murmured and pressed is one point in
//! the vowel space and two very different tones — so reading it separately is
//! the difference between six streams and five.
//!
//! **Each stream moves one thing, and only one.** Two streams driving one
//! parameter is one stream; one stream driving two parameters welds them
//! together so neither can move alone. What a listener hears as *variety* is how
//! many things can move independently of each other, which is exactly how many
//! streams reach something of their own — so the count above is the honest
//! measure of how much of a voice this mapping can hear.
//!
//! **The rule that keeps this from being resynthesis** is the same one as
//! everywhere else: the voice moves the law, not the notes. Nothing here plays
//! his pitch. His pitch bends a tuning system; his mouth chooses degrees within
//! it; the result is in his scale at his tonic, and is not the thing he said.

use utterance_analysis::voiceprint::Voiceprint;

use crate::compose::compose_noise;
use crate::params::{self, Params};
use crate::score::{Field, Score};
use crate::streams::{self, DRIFT_FRAMES, LEVEL_FRAMES, ROOT_FRAMES};
use crate::voice::Voice;

/// Voices the field sounds with when nobody says otherwise.
///
/// Enough that the result is a texture rather than a chord anyone counts, few
/// enough that each is separately audible. Now a default rather than a rule —
/// see [`Params`].
pub const VOICES: usize = 5;

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

    // Every stream is smoothed at its own timescale, which is the point: the
    // field moves at several rates at once, as a voice does.
    let drift = streams::smooth(&streams::filled(&vp.pitch.hz), DRIFT_FRAMES);
    let (open_raw, front_raw) = streams::vowel(vp, voice);
    let open = streams::smooth(&open_raw, ROOT_FRAMES);
    let front = streams::smooth(&front_raw, ROOT_FRAMES);
    let level = streams::smooth(&streams::level(vp), LEVEL_FRAMES);
    let bright = streams::smooth(&streams::brightness(vp, voice), ROOT_FRAMES);
    let depth = streams::smooth(&streams::depth(vp, voice), ROOT_FRAMES);
    // Flux is smoothed at the fastest timescale of any stream here. Slower and
    // it stops being articulation and becomes another loudness curve; this is
    // the one stream whose whole content is how quickly things are changing.
    let stir = streams::smooth(&vp.events.flux, LEVEL_FRAMES);

    let mut voices = vec![vec![0.0f32; frames]; params.voices];
    let mut gains = vec![vec![0.0f32; frames]; params.voices];
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
        let drift_octaves = (drift[i] / voice.tonic_hz).max(0.01).log2() * params.drift;
        let root_octaves = front[i].clamp(0.0, 1.0) * params.reach;
        let base = voice.tonic_hz * 2f32.powf(drift_octaves + root_octaves);

        // An open vowel spreads the voices apart, a closed one gathers them.
        let spread = 1.0 + open[i].clamp(0.0, 1.0);

        // The chord's shape, from the mouth shape the vowel chart cannot see.
        // Centred so that a speaker at the middle of their own F3 range is
        // stacked evenly and either half of it leans somewhere: high F3 opens
        // the top of the chord, low F3 pulls it into a cluster.
        let skew = (depth[i].clamp(0.0, 1.0) - 0.5) * 2.0 * params.voicing;
        let top = (params.voices - 1).max(1) as f32;

        for v in 0..params.voices {
            // Applied as a share of how far up the stack this voice is, so the
            // root never moves and the voicing opens from the top. A skew
            // applied evenly would just be `spacing` with extra steps.
            let lean = 1.0 + skew * (v as f32 / top);
            let raw = v as f32 * params.spacing as f32 * spread * lean;
            let step = (raw.max(0.0) as usize).min(choices * 2);
            let degree = degrees[step % choices];
            let octave = (step / choices) as f32;
            voices[v][i] = base * 2f32.powf(octave) * degree.ratio;

            // Upper voices fade in as the speaker gets louder, so a quiet
            // passage is a thinner texture and not merely a softer one.
            let reach = (level[i] * params.voices as f32) - v as f32;
            // ...and a moving mouth lifts them further, so a busy passage is a
            // busier texture. Weighted up the stack for the same reason the
            // voicing is: applied to every voice equally it would be loudness.
            let stirred = 1.0 + params.articulation * stir[i].clamp(0.0, 1.0) * (v as f32 / top);
            gains[v][i] =
                (level[i] * reach.clamp(0.0, 1.0) * stirred).max(if v == 0 { FLOOR } else { 0.0 });
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
///
/// The consonants come from the same place the note mapping gets them. They are
/// events by nature — a consonant is a thing that happens at a moment — so they
/// stay a list whether the pitched material is a field or a stream of notes.
pub fn score(vp: &Voiceprint, voice: &Voice) -> Score {
    score_with(vp, voice, Params::default())
}

/// The same, with the knobs set explicitly.
pub fn score_with(vp: &Voiceprint, voice: &Voice, params: Params) -> Score {
    let params = params.sane();
    let loudest = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    Score {
        duration_s: vp.source.duration_s,
        palette: voice.palette.clone(),
        detune_cents: voice.detune_cents,
        noise: compose_noise(vp, loudest, params.consonants),
        field: compose_with(vp, voice, params),
        events: Vec::new(),
    }
}
