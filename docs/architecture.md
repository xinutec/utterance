# Architecture

## The thesis

The voice is the **law-giver**, not the sound source. The recording is never
played back, pitch-shifted or resynthesised into the output. It is analysed into
a set of constraints, and the music is what those constraints generate.

The consequence to hold onto when making decisions: a listener hearing the output
should not hear a voice. They should hear a piece whose *rules* came from a
specific person.

## The three-way split

The single structural commitment of this repo. These layers stay in separate
crates so that a discarded aesthetic idea never drags analysis code with it.

```
audio ──▶ [ analysis ] ──▶ voiceprint ──▶ [ mapping ] ──▶ score ──▶ [ realisation ] ──▶ audio
          objective                       aesthetic                  mechanical
          deterministic                   swappable, many            one score, many renderings
          unit-testable                   of them coexisting
```

**analysis** (`music-analysis`) answers questions with right answers. Is this
frame voiced? What is f0 here? Where are the syllable onsets? It is testable
against fixtures because it can be wrong in a way you can demonstrate. It holds
no musical opinions whatsoever — it does not know what a scale is.

**mapping** answers questions with no right answers. Should this vowel be a minor
chord? It is where the art lives, and where we expect to write many competing
implementations over one stable voiceprint and keep the ones that sound good.
Because the voiceprint is a stable serialised document, a mapping can be
rewritten without re-analysing anything.

**realisation** turns a score into sound. Mechanical.

Violating this split — letting a mapping reach back into the audio, or letting
analysis emit notes — is the failure mode that ends the project, because it makes
every aesthetic experiment cost a DSP rewrite.

## The voiceprint

The interface between analysis and everything downstream, and the only artefact
that needs to stay stable. Serialised as JSON so it can be diffed, checked into
fixtures, and inspected in the browser without running the analyser.

Current fields are documented by the types in `music-analysis/src/voiceprint.rs`.
The intent for each:

- **f0 track** — prosodic contour. Gestural melody: rises, falls, declination.
  Not a tune, and should not be mapped to one naively.
- **energy envelope** — phrasing and breath groups.
- **voicing** — which frames carry a glottal source at all. Gates everything
  pitch-derived; unvoiced frames are consonant texture, not silence.
- **onsets** — event times. The raw material for a metrical grammar, not yet the
  grammar itself.

Planned, in rough order of how much they unlock (see `docs/roadmap.md`):
measured partial ratios (→ tuning), formant trajectories (→ harmony via vowel
space), stress hierarchy (→ meter), phone-class segmentation (→ symbol stream).

## Determinism

Analysis must be a pure function of the audio bytes. Same input, same voiceprint,
byte for byte, on any machine. This is what makes fixtures meaningful and what
lets us tell "I changed the mapping" apart from "the analyser drifted".

No clock, no randomness, no parallel reduction with nondeterministic ordering, no
dependence on floating-point library differences where avoidable.

## Why no ML

Not ideology — leverage. The structures we want (harmonic ratios, formant
geometry, metrical trees) are *already* explicit in classical DSP and phonology,
and a model that infers them would give us a number without the derivation. We
need the derivation, because the derivation is what the mapping layer reasons
over.
