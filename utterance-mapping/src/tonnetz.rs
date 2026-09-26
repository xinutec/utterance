//! Harmony as a walk across the speaker's own harmonic lattice.
//!
//! In `field` the voice stack slides with the vowel, so no chord stays long
//! enough to be heard in a tuning — that takes about a second. Here the vowel
//! space maps onto a lattice spanned by two of the speaker's consonances
//! ([`crate::lattice`]), and position is quantised to a triangle:
//!
//! - **Chords hold** while the mouth stays in one triangle, as every other
//!   stream keeps moving underneath.
//! - **Changes are small**: neighbouring triangles share two pitches, so the
//!   harmony holds two voices and steps one.
//!
//! Still continuous tracking: the harmony is quantised, not the time.

use utterance_analysis::voiceprint::Voiceprint;

use crate::compose::field_score;
use crate::field::voice_gain;
use crate::lattice::{Lattice, Triangle, Walk};
use crate::params::{self, Params};
use crate::score::{Field, Score};
use crate::streams::{self, DRIFT_FRAMES, LEVEL_FRAMES, ROOT_FRAMES};
use crate::voice::Voice;

/// Cells of lattice a vowel crosses at `reach = 1`: a handful of chords from one
/// extreme to the other, rather than one or dozens.
const CELLS_PER_REACH: f32 = 3.0;

/// Where one voice sits above the next at `spacing = 1`, in cents — a target:
/// each voice takes whichever octave of its pitch class is nearest its place.
/// Matched to the field mapping's register, so the two compare on harmony rather
/// than register; much closer would crowd the chord into mud.
const CLOSE_POSITION_CENTS: f32 = 300.0;

/// Widest the chord may be laid out across, in cents: four octaves, or twelve
/// widely spaced voices would climb out of hearing. Spacing is capped against
/// the voice count, so a small chord can still be open.
const MAX_SPAN_CENTS: f32 = 4800.0;

/// Least distance between two voices, in cents: below a quarter-tone two tones
/// are heard as one beating.
const MIN_SEPARATION_CENTS: f32 = 50.0;

/// How far the mouth shape may tip the chord's weight, at `voicing = 1`: the far
/// end drops to 40% and never silences, or the mouth would decide the voice
/// count.
const LEAN: f32 = 0.6;

/// Build the lattice field for a take.
pub fn compose(vp: &Voiceprint, voice: &Voice) -> Option<Field> {
    compose_with(vp, voice, Params::default())
}

/// Which triangle the harmony is in, frame by frame — the walk, separated from
/// the chord so `src/bin/dwell.rs` measures the real thing. `None` exactly where
/// [`compose_with`] is.
pub fn harmonic_path(vp: &Voiceprint, voice: &Voice, params: Params) -> Option<Vec<Triangle>> {
    let params = params.sane();
    // The speaker's own scale, never the bound one, so the walk is the same
    // whatever `bind` is. A plane-less scale's reason is reported by the route.
    Lattice::from_tuning(&voice.tuning).ok()?;
    if vp.frame.count == 0 {
        return None;
    }

    let (open_raw, front_raw) = streams::vowel(vp, voice);
    let open = streams::smooth(&open_raw, ROOT_FRAMES);
    let front = streams::smooth(&front_raw, ROOT_FRAMES);

    // Stateful — holding a chord means remembering it — and still deterministic.
    let span = CELLS_PER_REACH * params.reach;
    let position = |i: usize| {
        (
            (front[i].clamp(0.0, 1.0) - 0.5) * span,
            (open[i].clamp(0.0, 1.0) - 0.5) * span,
        )
    };
    let (x0, y0) = position(0);
    let mut walk = Walk::start(x0, y0);

    // Seconds to frames against this take's hop, rounded so the slider's bottom
    // step still means one frame.
    let dwell_frames = (params.settle / vp.frame.hop_s.max(f32::EPSILON)).round() as usize;

    Some(
        (0..vp.frame.count)
            .map(|i| {
                let (x, y) = position(i);
                walk.step(x, y, params.hold, dwell_frames)
            })
            .collect(),
    )
}

/// Build the lattice field with the knobs set explicitly, or `None` when the
/// speaker's scale spans no plane ([`Lattice::from_tuning`]).
pub fn compose_with(vp: &Voiceprint, voice: &Voice, params: Params) -> Option<Field> {
    let params = params.sane();
    // Laid out on the speaker's own scale; `bind` is applied to each sounding
    // pitch (`params::bind_cents_toward_equal` says why).
    let lattice = Lattice::from_tuning(&voice.tuning).ok()?;
    let path = harmonic_path(vp, voice, params)?;

    let frames = vp.frame.count;
    let drift = streams::smooth(&streams::filled(&vp.pitch.hz), DRIFT_FRAMES);
    let level = streams::smooth(&streams::level(vp), LEVEL_FRAMES);
    let bright = streams::smooth(&streams::brightness(vp, voice), ROOT_FRAMES);
    let depth = streams::smooth(&streams::depth(vp, voice), ROOT_FRAMES);
    let stir = streams::smooth(&vp.events.flux, LEVEL_FRAMES);

    let mut voices = vec![vec![0.0f32; frames]; params.voices];
    let mut gains = vec![vec![0.0f32; frames]; params.voices];
    let mut colour = vec![0.0f32; frames];
    let mut breath = vec![0.0f32; frames];

    for i in 0..frames {
        let here = path[i];

        // The speaker's prosody as a slow transposition, against their habitual
        // pitch as in `field`.
        let drift_octaves = (drift[i] / voice.tonic_hz).max(0.01).log2() * params.drift;
        let base = voice.tonic_hz * 2f32.powf(drift_octaves);

        // F3 tips the chord's weight rather than its spelling: pitch here moves
        // in lattice steps, so F3 reaching the harmony would be silent and then
        // jump, where balance is continuous. Centred on the speaker's F3 range.
        let lean = (depth[i].clamp(0.0, 1.0) - 0.5) * 2.0 * params.voicing;
        let top = (params.voices - 1).max(1) as f32;
        let gap = (CLOSE_POSITION_CENTS * params.spacing as f32).min(MAX_SPAN_CENTS / top);

        // Absolute pitch classes, so a pitch two chords share is one frequency.
        // Bound here, on the note that sounds: each moves at most a quarter tone
        // and the chord is untouched.
        let mut pitch_classes: Vec<f32> = here
            .ring(params.voices)
            .into_iter()
            .map(|(x, y)| params::bind_cents_toward_equal(lattice.pitch_class(x, y), params.bind))
            .collect();
        pitch_classes.sort_by(f32::total_cmp);

        let mut previous = f32::NEG_INFINITY;
        for v in 0..params.voices {
            let pc = pitch_classes.get(v).copied().unwrap_or(0.0);
            // Register from the pitch class alone, not the chord: each voice
            // takes the octave nearest its place, so a kept pitch keeps its
            // frequency. Stacking from each chord's lowest note would
            // re-register everything on every change.
            let target = v as f32 * gap;
            let placed = pc + 1200.0 * ((target - pc) / 1200.0).round();
            // Never two voices within a quarter-tone: whole octaves up until it
            // clears, in closed form so the bound is in the arithmetic.
            let floor = previous + MIN_SEPARATION_CENTS;
            let cents = if placed < floor {
                placed + 1200.0 * ((floor - placed) / 1200.0).ceil()
            } else {
                placed
            };
            previous = cents;
            voices[v][i] = base * 2f32.powf(cents / 1200.0);

            // Weight tipped toward the top or bottom of the chord, pivoting on
            // the middle so overall loudness stays the envelope's.
            let weighted = (1.0 + LEAN * lean * (v as f32 / top - 0.5) * 2.0).max(0.0);
            gains[v][i] = voice_gain(level[i], stir[i], v, &params, weighted);
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

/// A whole score for this take: the lattice field, plus the speaker's consonants.
pub fn score(vp: &Voiceprint, voice: &Voice) -> Score {
    score_with(vp, voice, Params::default())
}

/// The same, with the knobs set explicitly.
pub fn score_with(vp: &Voiceprint, voice: &Voice, params: Params) -> Score {
    let params = params.sane();
    field_score(vp, voice, params, compose_with(vp, voice, params))
}
