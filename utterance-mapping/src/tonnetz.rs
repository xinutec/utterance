//! Harmony as a walk across the speaker's own harmonic lattice.
//!
//! The other continuous mapping, and the answer to the thing `field` cannot do.
//! There, five voices are stacked at a fixed distance in scale degrees and the
//! whole stack slides as the vowel moves: every moment is the same chord at a
//! different pitch, and it is never in one place long enough to be heard as
//! being in a tuning at all. Measured rather than supposed — `docs/roadmap.md`
//! records the derived scale as real and currently inaudible, because a chord
//! has to ring for about a second before anyone can hear whether its partials
//! lock or beat.
//!
//! **What changes here.** The two dimensions of vowel space become the two
//! dimensions of a lattice spanned by the speaker's two deepest consonances (see
//! [`crate::lattice`]), and the position on it is *quantised to a triangle*.
//! Two consequences, and the second is the point:
//!
//! - **Chords hold.** While the mouth stays inside one triangle the pitches do
//!   not move at all, so a sustained vowel is a sustained chord. Everything else
//!   the voice does — loudness, tone colour, breath, the slow pitch drift — goes
//!   on moving underneath it, so holding still harmonically is not holding still.
//! - **Changes are small.** Triangles that share an edge share two of their
//!   three pitches, so the harmony moves by holding two voices and stepping one.
//!   Nobody wrote that rule; it is what adjacency on this lattice *is*.
//!
//! **This is still continuous tracking**, which is the trade the project chose:
//! nothing is cut into events and no frame is skipped. What is quantised is the
//! harmony, not the time — the chord changes when the mouth changes, at whatever
//! irregular moment that happens to be.
//!
//! The knobs mean what they mean everywhere else, read onto this geometry:
//! `reach` is how much of the lattice a vowel crosses, `spacing` how open the
//! chord is voiced, `voicing` how far the mouth shape tips the chord's weight,
//! and `hold` how far past a boundary the mouth must go before the harmony
//! follows.

use utterance_analysis::voiceprint::Voiceprint;

use crate::compose::compose_noise;
use crate::lattice::{Lattice, Triangle, settle, triangle_at};
use crate::params::{self, Params};
use crate::score::{Field, Score};
use crate::streams::{self, DRIFT_FRAMES, LEVEL_FRAMES, ROOT_FRAMES};
use crate::voice::Voice;

/// Cells of lattice a vowel crosses at `reach = 1`.
///
/// Three, so a mouth moving from one extreme to the other passes through a
/// handful of chords rather than one or dozens. Fewer and a whole utterance is
/// one harmony; more and the quantising buys nothing back, because the position
/// crosses a boundary as often as a continuous root would have moved.
const CELLS_PER_REACH: f32 = 3.0;

/// Where one voice sits above the next at `spacing = 1`, in cents.
///
/// A target rather than a rule: each voice takes whichever octave of its pitch
/// falls nearest to its own place in the register. A quarter of an octave, so
/// the closest a chord is ever voiced still spreads five voices across one —
/// near enough for two voices' partials to beat against each other, which is
/// the whole point of holding a chord still.
///
/// Set by measuring against the other mapping rather than by taste. At half
/// this, the default chord occupied 113–237 Hz where the field mapping's
/// occupied 122–928: five voices inside one octave at the bottom of a man's
/// range, which is mud whatever its tuning. Two mappings meant to be compared
/// have to sit in the same register or the comparison is about register.
const CLOSE_POSITION_CENTS: f32 = 300.0;

/// Widest the chord is allowed to be laid out across, in cents.
///
/// Four octaves, a little past what the field mapping's own stacking reaches.
/// Without it, twelve voices at the widest spacing would target more than eight
/// octaves above the tonic and the upper ones would leave the range a person
/// hears — a voice count that silently stops meaning voices. Spacing is capped
/// against the voice count rather than in itself, so a small chord can still be
/// as open as it likes.
const MAX_SPAN_CENTS: f32 = 4800.0;

/// Least distance between two voices, in cents.
///
/// A quarter-tone. Below this two tones are not heard as two notes but as one
/// beating, so a chord that puts a pair here is quietly a voice short.
const MIN_SEPARATION_CENTS: f32 = 50.0;

/// How far the mouth shape may tip the chord's weight, at `voicing = 1`.
///
/// 0.6, so an extreme leaves the far end of the chord at 40% and never silences
/// it. A voicing that can mute a voice outright would make the voice count
/// something the mouth decides, which is a different knob wearing this one's
/// name.
const LEAN: f32 = 0.6;

/// Quietest the field ever falls, relative to its loudest moment.
///
/// The same floor `field` keeps, and for the same reason: a field that stops is
/// a sequence of events again.
const FLOOR: f32 = 0.02;

/// Build the lattice field for a take.
pub fn compose(vp: &Voiceprint, voice: &Voice) -> Option<Field> {
    compose_with(vp, voice, Params::default())
}

/// Which triangle the harmony is in, frame by frame.
///
/// The walk itself, separated from the chord it is turned into. Public because
/// it is the thing worth measuring: *how long a chord rings* is the question
/// this whole mapping exists to answer, and a tool that reimplemented the walk
/// to find out could drift from it and report on a mapping nobody is listening
/// to. `src/bin/dwell.rs` reads this.
///
/// Returns `None` in exactly the cases [`compose_with`] does.
pub fn harmonic_path(vp: &Voiceprint, voice: &Voice, params: Params) -> Option<Vec<Triangle>> {
    let params = params.sane();
    // **The speaker's own scale, never the bound one.** See `compose_with` for
    // why the axes must not follow `bind`; here it also means the walk itself is
    // the same walk whatever the tuning, so two settings can be compared without
    // comparing two different pieces.
    //
    // The reason it spans no plane is not thrown away here so much as asked for
    // somewhere else: `routes::api` calls `Lattice::from_tuning` itself, so a
    // refusal reaches the browser as a sentence rather than as silence.
    Lattice::from_tuning(&voice.tuning).ok()?;
    if vp.frame.count == 0 {
        return None;
    }

    let (open_raw, front_raw) = streams::vowel(vp, voice);
    let open = streams::smooth(&open_raw, ROOT_FRAMES);
    let front = streams::smooth(&front_raw, ROOT_FRAMES);

    // The walk is stateful, because holding a chord means remembering which one
    // is being held. Deterministic all the same: the state is a pure function of
    // the frames before it, and it starts wherever the first frame lands.
    let span = CELLS_PER_REACH * params.reach;
    let position = |i: usize| {
        (
            (front[i].clamp(0.0, 1.0) - 0.5) * span,
            (open[i].clamp(0.0, 1.0) - 0.5) * span,
        )
    };
    let (x0, y0) = position(0);
    let mut here: Triangle = triangle_at(x0, y0);

    Some(
        (0..vp.frame.count)
            .map(|i| {
                let (x, y) = position(i);
                here = settle(here, x, y, params.hold);
                here
            })
            .collect(),
    )
}

/// Build the lattice field with the knobs set explicitly.
///
/// Returns `None` when the speaker's scale spans no plane — see
/// [`Lattice::from_tuning`]. That is a real answer rather than a failure: a
/// scale of the fifth and nothing else has one axis, and laying a lattice over
/// it anyway would mean one of the two vowel dimensions silently reaching
/// nothing.
pub fn compose_with(vp: &Voiceprint, voice: &Voice, params: Params) -> Option<Field> {
    let params = params.sane();
    // **The lattice is laid out on the speaker's own scale, and `bind` is
    // applied to the notes it produces rather than to its axes.**
    //
    // Binding the axes was the first arrangement and it made `bind` untestable
    // here. A point's pitch is `x·a + y·b`, so moving an axis moves a chord near
    // the tonic by a few cents and one three cells out by fifty or more — and
    // once that folds into an octave, far enough out it is a different note. Two
    // renders differing in `bind` were therefore two different chord sequences,
    // and comparing their tuning was comparing nothing.
    //
    // Measured: `src/bin/beating.rs` had equal temperament beating *less* than
    // the derived scale on this mapping, the reverse of the claim and of what
    // the field mapping shows, because the structural change swamped the tuning.
    // Applied per sounding pitch, `bind` moves every note by at most a quarter
    // tone and leaves the chord it belongs to alone.
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

        // His prosody, as a slow transposition of everything — measured against
        // the speaker's habitual pitch rather than this take's own median, for
        // the reason recorded in `field`.
        let drift_octaves = (drift[i] / voice.tonic_hz).max(0.01).log2() * params.drift;
        let base = voice.tonic_hz * 2f32.powf(drift_octaves);

        // The mouth shape the vowel chart cannot see decides where the chord's
        // weight sits. Centred so a speaker in the middle of their own F3 range
        // leans neither way.
        //
        // **Weight rather than spelling**, which is where this parts company
        // with the field mapping, and the reason is the geometry rather than
        // taste. Everything about pitch here is a lattice point, so anything F3
        // reached through the harmony would move in steps — silent for most of
        // the knob's travel and then a jump. A stream that only registers at a
        // threshold is a stream barely read. Balance across the chord is
        // continuous, so the third formant is audible everywhere along it.
        let lean = (depth[i].clamp(0.0, 1.0) - 0.5) * 2.0 * params.voicing;
        let top = (params.voices - 1).max(1) as f32;
        let gap = (CLOSE_POSITION_CENTS * params.spacing as f32).min(MAX_SPAN_CENTS / top);

        // Absolute pitch classes, not intervals above a moving root. That is
        // what makes two adjacent chords share tones *in sound* rather than only
        // on paper: a pitch the lattice keeps is a frequency the ear keeps.
        // Bound here, at the last moment, on the note that will actually sound.
        // Every pitch moves by at most a quarter tone toward the grid everyone
        // else uses, and the chord it belongs to — which lattice points, in what
        // order, held for how long — is untouched. That is what makes `bind` a
        // tuning knob on this mapping rather than a structural one.
        let mut pitch_classes: Vec<f32> = here
            .ring(params.voices)
            .into_iter()
            .map(|(x, y)| params::bind_cents_toward_equal(lattice.pitch_class(x, y), params.bind))
            .collect();
        pitch_classes.sort_by(f32::total_cmp);

        let mut previous = f32::NEG_INFINITY;
        for v in 0..params.voices {
            let pc = pitch_classes.get(v).copied().unwrap_or(0.0);
            // **Register from the pitch class alone, not from the chord it is
            // in.** Each voice has a place it wants to sit and takes whichever
            // octave of its pitch is nearest to it, so a pitch the lattice keeps
            // across a chord change keeps its *frequency* too and the voice
            // holding it does not move at all. Stacking each chord from its own
            // lowest note instead — the obvious way — re-registers everything
            // whenever the set changes, and the common tones the geometry went
            // to such trouble to provide are then audible nowhere.
            let target = v as f32 * gap;
            let mut cents = pc + 1200.0 * ((target - pc) / 1200.0).round();
            // Two voices on one pitch are one voice twice as loud, which sounds
            // thinner than the voice count claims. This is the only place the
            // rest of the chord gets a say, and it is a floor rather than a
            // layout.
            while cents < previous + MIN_SEPARATION_CENTS {
                cents += 1200.0;
            }
            previous = cents;
            voices[v][i] = base * 2f32.powf(cents / 1200.0);

            // Loudness and articulation behave exactly as in `field`, so the two
            // mappings differ in their harmony and in nothing else — which is
            // the only way comparing them says anything.
            let reach = (level[i] * params.voices as f32) - v as f32;
            let stirred = 1.0 + params.articulation * stir[i].clamp(0.0, 1.0) * (v as f32 / top);
            // Weight tipped toward the top of the chord or the bottom of it,
            // pivoting on the middle so the chord's overall loudness is left to
            // the energy envelope where it belongs.
            let weighted = (1.0 + LEAN * lean * (v as f32 / top - 0.5) * 2.0).max(0.0);
            gains[v][i] = (level[i] * reach.clamp(0.0, 1.0) * stirred * weighted).max(if v == 0 {
                FLOOR
            } else {
                0.0
            });
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
