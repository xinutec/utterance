# Architecture

## The thesis

The voice is the **law-giver**, not the sound source. The recording is never
played back, pitch-shifted or resynthesised into the output. It is analysed into
a set of constraints, and the music is what those constraints generate.

The consequence to hold onto when making decisions: a listener hearing the output
should not hear a voice. They should hear a piece whose *rules* came from a
specific person.

**The singer controls the law, not the notes.** Sing higher and the tuning system
stretches; it is not that a higher note plays. This is the same commitment stated
from the performer's side, and it carries a design constraint with it.

A voice emits on the order of ten independent continuous streams at once — f0,
loudness, F1, F2, F3, spectral tilt, breathiness, jitter, plus voicing as a
discrete gate. A note has three: pitch, loudness, duration. So an output built
out of notes has nowhere to put six or seven of those streams, and discards them.
That is the whole reason voice-driven synthesisers feel like a gimmick — the
controller is richer than the thing it controls — and avoiding it is a constraint
on the mapping and realisation layers, not a matter of taste. **Whatever the
music is made of has to need as many hands to play as a voice has.**

The property being spent here is one voices have and instruments mostly do not:
pitch and timbre are independent. On an acoustic instrument the spectrum follows
the pitch and the dynamics whether you want it to or not, while a voice can hold
f0 dead still and sweep the vowel across its whole space. Two orthogonal
controllers in one organ, which is what makes formants a separate measurement
rather than another view of pitch.

## The three-way split

The single structural commitment of this repo. These layers live in separate
crates so that a discarded aesthetic idea never drags analysis code with it.

All three exist. The dependency runs one way only — realisation reads a score,
mapping reads a voiceprint, and neither lower layer may learn that a higher one
exists. `src` is the composition root and the only crate that depends on all
three.

The **score** is the second stable interface, alongside the voiceprint, and it
carries absolute frequencies in hertz: no degrees, no scale, no key. That is the
mirror of the rule keeping analysis from knowing what a scale is. By the time a
score exists every musical decision is already made, which is what lets a
synthesiser be rewritten without touching a mapping.

**What a score carries is the ceiling on how the music can sound**, and it has
been widened twice for exactly that reason. It began as four numbers per note
plus one fixed spectrum, and no synthesiser craft could get past that — a
spectrum that cannot change produces a tone that does not move. It now carries a
palette of spectra to travel between, a colour per note, breath, the speaker's
own detune, a stream of noise events for the consonants, and a `Field`: per-frame
parameter streams for music that is not made of events at all.

Widening this interface is how the output gets richer. Tinkering downstream of it
is not.

```
audio ──▶ [ analysis ] ──▶ voiceprint ──▶ [ mapping ] ──▶ score ──▶ [ realisation ] ──▶ audio
          objective                       aesthetic                  mechanical
          deterministic                   swappable, many            one score, many renderings
          unit-testable                   of them coexisting
```

**analysis** (`utterance-analysis`) answers questions with right answers. Is this
frame voiced? What is f0 here? Where are the syllable onsets? It is testable
against fixtures because it can be wrong in a way you can demonstrate. It holds
no musical opinions whatsoever — it does not know what a scale is.

**mapping** (`utterance-mapping`) answers questions with no right answers. Should
this vowel be a minor chord? It is where the art lives, and where we expect to
write many competing implementations over one stable voiceprint and keep the ones
that sound good. Because the voiceprint is a stable serialised document, a
mapping can be rewritten without re-analysing anything.

What is testable in mapping is narrower than in analysis, and worth stating so
nobody mistakes a passing suite for a musical judgement: the arithmetic can be
checked against results the literature already establishes, and the derivation
can be checked to be reading its input rather than restating its own assumptions
— a stretched spectrum must *not* make the octave consonant. Whether the output
sounds good is not a thing any test here decides.

**Mappings coexist rather than replace each other.** Two exist: `compose` reads
onsets and emits notes, `field` reads every frame and emits a continuously
sounding texture. They are alternatives over one voiceprint, renderable
separately or together, because the only way to judge either is against the
other. Anything a mapping chooses that could reasonably be chosen differently
belongs in `params::Params` and is reachable from the render URL — a constant can
only be changed by editing and rebuilding, and these are meant to be swept by
whoever is listening. `params::KNOBS` describes each one well enough to be
offered as a control — range, step, starting value, and a line saying what it
does — and the server publishes that table at `GET /api/controls`, so the
sliders in the browser are generated from the crate that obeys them rather than
maintained alongside it.

**realisation** (`utterance-realisation`) turns a score into sound. Mechanical, and
additive rather than sampled — forced rather than chosen, because a derived
tuning puts notes wherever the speaker's spectrum says they belong and no sampled
instrument can play 582 cents.

It renders the score's own timbre rather than one of its choosing. A scale
derived from a spectrum is only consonant for tones that *have* that spectrum;
synthesise anything else and the roughness minima stop lining up with the notes,
so the scale keeps its numbers and loses its justification.

Violating this split — letting a mapping reach back into the audio, or letting
analysis emit notes — is the failure mode that ends the project, because it makes
every aesthetic experiment cost a DSP rewrite.

## The voiceprint

The interface between analysis and everything downstream, and the only artefact
that needs to stay stable. Serialised as JSON so it can be diffed, checked into
fixtures, and inspected in the browser without running the analyser.

Current fields are documented by the types in `utterance-analysis/src/voiceprint.rs`.
The intent for each:

- **f0 track** — prosodic contour. Gestural melody: rises, falls, declination.
  Not a tune, and should not be mapped to one naively.
- **energy envelope** — phrasing and breath groups.
- **voicing** — which frames carry a glottal source at all. Gates everything
  pitch-derived; unvoiced frames are consonant texture, not silence.
- **onsets** — event times. The raw material for a metrical grammar, not yet the
  grammar itself. Read them as *the spectrum changed here*, which is what was
  measured, not as *a syllable began here*, which is what we want and cannot yet
  distinguish — a continuously glided vowel produces the former without the
  latter. Separating them needs the stress hierarchy, below.
- **formants** — F1, F2 and F3, the vocal-tract resonances. F1 against F2 is a
  two-dimensional space in which every vowel of a language occupies a region, so
  a vowel sequence is a path through it — the geometry the harmony mapping is to
  be built on. Nearly independent of pitch, which is what makes it a separate
  measurement rather than a view of the same thing. `null` where the fit found
  nothing in that formant's anatomical range: assignment is per-frame with no
  continuity tracking, so a formant that drops out is reported as absent rather
  than filled in from the one above it.

- **partials** — the take's measured harmonic series, where it held a pitch long
  enough to have one. Not a per-frame series: it describes the recording as a
  whole. The amplitudes are the payload rather than the frequencies, because a
  voice is very nearly harmonic and what differs between people is which
  partials their tract emphasises. This is what a tuning is derived from.
- **texture** — spectral centroid and flatness per frame, measured above 300 Hz.
  Defined everywhere, interesting mostly where the voice is *unvoiced*: nearly
  three quarters of ordinary speech carries no fundamental, and every other field
  here gates that away. It characterises noise rather than classifying phones —
  knowing a frame is an /s/ is a linguistic label, knowing its energy sits at
  7 kHz across a wide band is what a synthesiser can act on.

Planned, in rough order of how much they unlock (see `docs/roadmap.md`): stress
hierarchy (→ meter), phone-class segmentation (→ symbol stream).

## The speaker profile

A second analysis artefact, and a deliberately different object from a
voiceprint: `utterance-analysis/src/speaker.rs` measures the **person**, where a
voiceprint measures one **utterance**.

The speaker is the world; the utterance is the piece. Pitch range and the corners
of a vowel space are anatomy and habit — they barely move between takes — and
they are what a tuning system and a harmonic lattice have to be built from. What
was said decides only what happens inside that. Keeping the two in separate
documents is what stops a mapping deriving a speaker's range from one short take
that never reached it.

It is measurement rather than aesthetics, which is why it lives in the analysis
layer: *how high does this person's F2 go* has an answer that can be shown wrong.
Two properties are worth knowing from outside the module:

- **Bounds are percentiles, not extremes.** Formant assignment is per-frame with
  no continuity tracking, so a few frames per take land somewhere the speaker
  never was, and a true minimum and maximum would be defined entirely by those
  frames.
- **A range is withheld rather than guessed** when there is too little material
  to measure it, with the frame counts still reported so a caller can tell "too
  little" from "none".
- **Everything a mapping normalises against belongs here.** Vowel-space corners,
  the third formant's range, pitch, and the brightness the speaker's voiced tone
  moves through. The rule is not about tidiness: normalised against a fixed
  range, a measurement stops meaning *bright for them*; normalised against the
  take, the difference between two things one person said is normalised away —
  and that second failure has already shipped once, in the field's pitch drift.

Because a profile is a pure function of the voiceprints it is built from, it is a
cache in the same sense they are, and carries its own version for the same
reason.

## Fixtures and ground truth

A measurement is only worth as much as the material it was judged on, and the two
kinds of recording we have answer different questions.

**Sustained material** — a held or glided vowel — bounds how badly something
over-fires. It cannot say what the right answer is, because a continuous sound
has no discrete events while still changing spectrally throughout. The onset
detector was first tuned against a synthetic sustained tone, passed cleanly, and
then reported 22 events in seven seconds of one real held vowel: a generated tone
has none of the jitter a voice does, so the fixture could not fail.

**Speech** is where accuracy has to be judged, and judging it needs labels a
person supplies by listening. We do not have those yet, so the onset tests assert
bounds rather than counts, and say so.

The rule this leaves behind: **when a test cannot fail, say so in the test.** A
bound honestly labelled as a bound is useful. The same assertion dressed up as
ground truth is worse than nothing, because it stops anyone looking again.

## Determinism

Analysis must be a pure function of the audio bytes. Same input, same voiceprint,
byte for byte. This is what makes fixtures meaningful and what lets us tell "I
changed the mapping" apart from "the analyser drifted".

No clock, no randomness, no parallel reduction with nondeterministic ordering,
and iterative solvers start from fixed points rather than anywhere that varies
between runs.

**Scope of the guarantee: one toolchain on one platform.** The analysis path is
full of `sin`, `ln`, `log10` and complex `arg`, and libm implementations differ
between platforms in the last bits — so byte-identical output *across* machines
is neither claimed nor tested. The determinism tests compare two runs in the same
process, which is exactly as far as the claim goes. Committing a golden
voiceprint and asserting against it would be worth doing, and would have to allow
a tolerance rather than compare bytes.

## Why no ML

Not ideology — leverage. The structures we want (harmonic ratios, formant
geometry, metrical trees) are *already* explicit in classical DSP and phonology,
and a model that infers them would give us a number without the derivation. We
need the derivation, because the derivation is what the mapping layer reasons
over.
