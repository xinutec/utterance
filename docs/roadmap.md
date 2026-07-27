# Roadmap

What is built, what is next, and the decisions already taken so they are not
re-litigated. Read `architecture.md` first — this assumes the three-way split and
the voiceprint as the interface between its layers.

## Where the analysis layer stands

| Measurement | State | Notes |
| --- | --- | --- |
| f0 contour | done | YIN. Guards the classic octave error (`tests/f0.rs`). |
| energy envelope | done | |
| voicing | done | Gates everything pitch-derived and the formants. |
| events (spectral flux) | done, with a known limit | See "onsets" below. |
| formants F1/F2/F3 | done | LPC + Durand-Kerner. Range-constrained assignment. |
| speaker profile | done | Per-person vowel-space corners and f0 range. |
| measured partial ratios | done | Per take, over frames steady enough to use. |
| stress hierarchy | **not started** | Needed for meter, and to fix onsets. |
| phone-class segmentation | **not started** | Needed for the symbol stream. |

All three layers exist. The chain runs end to end for the first time: a
calibration take yields a scale and a timbre, an utterance yields a score, the
score renders to audio. What that first render sounds like is the open question
below.

## The four mappings

In rough order of how much each unlocks.

1. **Tuning from measured partials.** *Built* — `music-mapping/src/tuning.rs`.
   A Plomp–Levelt roughness curve over the speaker's own measured spectrum,
   swept from unison to the octave, with its deep minima read as scale degrees.
   What came out of the first real calibration set:

   - **Reproducible across takes.** Two steady-*ah* recordings gave scales
     identical to the cent, despite individual partials differing by up to 8 dB
     between them. The curve integrates over every pair, so per-partial wobble
     barely moves it. Tuning needs no pooling across takes.
   - **Not reproducible across vowels.** The same speaker's *ah*, *ee* and *oo*
     gave 8, 3 and 4 degrees. A deliberately open *ah* yields a nearly-just
     scale — 6:5, 5:4, 7:5, 3:2, 8:5, 5:3 — while a relaxed one collapses to the
     fifth alone. See the open question below.
   - **Audibly not 12-TET.** Four of those six degrees sit 13–18 cents off
     equal temperament, and 7:5 at 582 cents has no equal-tempered equivalent.

2. **Harmony from vowel space.** F1/F2 is a 2D manifold; the Tonnetz is a 2D
   lattice. Map one onto the other and a sentence becomes a chord progression.
   Not started as harmony. `compose.rs` currently spends the same measurement on
   something far cruder — frontness picks a scale degree, openness picks an
   octave — which is one note where this wants a progression. That crudeness is
   deliberate: it was the shortest path to hearing anything.
3. **Meter from stress hierarchy.** Nested strong/weak grouping from syllable
   prominence. Blocked on stress measurement, which is also what onsets need.
4. **Development from the symbol stream.** Phone classes as an alphabet for a
   deterministic rewrite system. The furthest out and the least specified.

## Known gaps, with their cost

- **Onsets mean "the spectrum changed", not "a syllable began."** Spectral flux
  cannot separate the two; a continuously glided vowel demonstrates it. Threshold
  tuning needs speech with syllables labelled by ear, which no one has produced
  yet, so the onset tests assert bounds rather than counts. The real fix is the
  stress hierarchy, which carries a cue flux does not.
- **No formant continuity tracking.** Assignment is per-frame, constrained by
  anatomical range. Where a formant drops out of the fit its slot is nulled
  rather than filled with the one above — correct, but lossy. A Viterbi pass
  across frames would recover many of those frames.
- **Vowel-space landmarks are generic.** The reference positions drawn on the
  plot are population values for an adult speaker, not this speaker's. The
  calibration take is exactly what would fix that, and mapping (2) will need it
  done properly rather than as decoration.
- **Analysis is no longer cheap.** Measuring partials runs a 2048-point FFT on
  every steady frame, so a fifteen-second take costs seconds rather than
  milliseconds. Acceptable because it runs once per recording and the result is
  cached, but it is the reason a re-analysis sweep after a schema bump is now
  something you wait for.
- **Nothing derived is visible or audible in the UI.** Partials, the derived
  scale, the score and the rendered audio all exist only in the crates. The
  studio still shows pitch, energy, events and vowel space, and there is no way
  to hear a render from a browser — which is the thing the person supplying the
  voice most needs.
- **The first mapping is a placeholder.** `compose.rs` reads rhythm from onsets,
  which mean *the spectrum changed*, not *a syllable began*. Until the stress
  hierarchy exists the rhythm is wrong in a way no amount of taste in the mapping
  will fix.

## Decisions taken

Recorded so they are not reopened without reason. **This is not the full
ledger** — most decisions are documented at the code they govern, where they are
harder to forget about. Listed here are the ones with no obvious home in the
source, plus the one that most shapes daily work.

- **A stored voiceprint is a cache, not a record.** The audio is the source of
  truth and analysis is a pure function of it, so `SCHEMA_VERSION` identifies the
  analyser and `Store::ensure_current` re-derives anything stale. Bump that
  version for *any* change to the output, algorithm as much as shape: a shape
  change fails loudly on deserialise, an algorithm change is silent. This is why
  improving the analyser never invalidates a recording.

- **Capture stays in the browser** (2026-07-27). Server-side capture — the Mac
  recording from its own microphone with a phone as remote control — was
  considered and rejected. It would have sidestepped the secure-context limit,
  but it moves audio capture into the backend and doubles the number of capture
  paths to maintain.
- **Consequence: recording happens at the Mac.** Browsers only allow microphone
  access in a secure context, so `localhost` works and a plain-HTTP LAN address
  does not. A phone can browse takes and view voiceprints over the LAN; it cannot
  record. Serving over HTTPS would lift that if it ever matters.
- **No ML.** See `architecture.md`. The mapping layer needs the derivation, not
  an inferred number.

- **Aesthetic parameters live in the mapping layer.** Any control over how
  strongly the voice binds the result is a mapping parameter and never an
  analysis one. This follows from the split rather than being a fresh choice, but
  it has a practical edge worth stating: because a voiceprint is a cache keyed on
  the analyser, a knob in analysis would invalidate every recording each time it
  moved, while a knob in mapping can be swept against a fixed voiceprint and
  heard immediately.

- **A vowel space is normalised against the speaker's own extremes**
  (2026-07-27), not against population norms. It makes the *utterance* the
  variable rather than the anatomy, which is what lets a body of work by one
  person be one sound world with a different piece in each take. The cost is a
  calibration recording per speaker, which is cheap and which
  `music-analysis/src/speaker.rs` now consumes.

- **Control is exercised by learning the mapping, not by playing it live**
  (2026-07-27). Real-time would force the analysis to become causal and
  low-latency, giving up non-causal pitch tracking and whole-take statistics —
  a real loss of measurement quality, paid before anyone knows whether the
  mapping is worth performing. Deterministic mapping plus fast iteration gets
  most of the control for none of that: record, hear it, adjust, sing it again.

  Determinism is what makes this work, and it is the strongest argument for the
  no-ML decision that was not apparent when that decision was taken — a singer
  can only build a mental model of a system that answers the same way twice.

  **Constraint kept while this holds:** mapping should avoid depending on
  statistics of the whole take where it can, staying frame-local, so real-time
  remains reachable later. The speaker profile is not a violation: it is measured
  once per person and then fixed, not recomputed per take.

## Open questions

- **Should a listener be able to perceive the connection back to the voice?**
  Not yet answered. It is the largest single constraint on the mapping layer: a
  perceptible link forces mappings to stay legible, while a private seed frees
  them to be arbitrarily abstract. Current lean is perceptible, on the grounds
  that it is what makes the project legible to anyone but its authors — but this
  should be decided deliberately before mapping work starts, not defaulted into.

  Note that this is *not* the same axis as the one below, and the two may run
  opposite. Binding hard to the speaker yields the most derived music and the
  least speech-like, because a real voice's ratios are unfamiliar; binding
  loosely — a pitch contour quantised to an ordinary scale — is obviously
  melodised speech while barely transforming anything.

- **How far should the voice be allowed to bind?** The axis is convention ↔
  speaker: 12-TET, regular meter and ordinary voice leading at one end, the
  speaker's measured ratios alone at the other. Moving along it is the same
  operation in each dimension — snap toward a cultural grid by some amount:
  interpolate in cents between the derived scale degree and the nearest tempered
  one, between a measured tempo ratio and the nearest small-integer one, between
  the vowels' Tonnetz path and the nearest diatonic waypoint. One scalar per
  dimension, not one global scalar; binding tuning hard while leaving rhythm
  conventional is a different and probably more listenable result than moving
  both together.

  Unresolved is where on that axis the music actually is, which is an argument
  for building the control early and as an instrument for answering the question
  by ear, rather than shipping it as a slider and letting it stand in for a
  decision.
- **How should a speaker's vowel space be normalised?** Against their own
  measured extremes, or against population norms? Affects whether two people
  produce comparable music or merely internally consistent music.

- **Which vowel does a speaker's tuning come from?** Measured, not speculative:
  one person's *ah* gave an eight-note nearly-just scale and their *ee* gave the
  fifth and nothing else. A harmonic series belongs to a tract shape, so "the
  speaker's scale" is undefined until the vowel is pinned down. Candidates are a
  single nominated calibration vowel, the union of several, or a scale that
  changes with the vowel being sung — the last being the most interesting and the
  most likely to be unusable.

  Bound up with this: `tuning::MIN_DEPTH` decides when a dip in the roughness
  curve counts as a note, and part of that 8-versus-3 spread is the threshold
  rather than the voice. It should be settled by listening, not by argument.

  Until it is settled, `src/voice.rs` picks the take yielding the richest scale
  and `?calibration=<id>` overrides that. The first criterion tried — most steady
  frames — picked an eleven-second *ee* measured over a thousand frames whose
  scale is the fifth and nothing else, in preference to a five-second *ah*
  yielding eight degrees. Measurement quality and musical usefulness turned out
  to point in opposite directions, which is itself an argument that this question
  has to be answered deliberately.
