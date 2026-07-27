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
| measured partial ratios | **not started** | Needed for tuning. |
| stress hierarchy | **not started** | Needed for meter, and to fix onsets. |
| phone-class segmentation | **not started** | Needed for the symbol stream. |

Nothing in the mapping or realisation layers exists yet. That is deliberate: the
voiceprint had to be worth mapping first.

## The four mappings

In rough order of how much each unlocks. None is started.

1. **Tuning from measured partials.** Derive a scale from the speaker's own
   harmonic series rather than from 12-TET. Needs partial-ratio measurement,
   which needs clean unclipped sustained phonation — the calibration take is the
   right material. The most distinctive idea in the project and the one most
   likely to sound strange in an interesting way.
2. **Harmony from vowel space.** F1/F2 is a 2D manifold; the Tonnetz is a 2D
   lattice. Map one onto the other and a sentence becomes a chord progression.
   **Ready to start** — formants are measured and validated. Needs a decision
   about normalising the speaker's vowel space against their own extremes rather
   than against population averages.
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

## Open questions

- **Should a listener be able to perceive the connection back to the voice?**
  Not yet answered. It is the largest single constraint on the mapping layer: a
  perceptible link forces mappings to stay legible, while a private seed frees
  them to be arbitrarily abstract. Current lean is perceptible, on the grounds
  that it is what makes the project legible to anyone but its authors — but this
  should be decided deliberately before mapping work starts, not defaulted into.
- **How should a speaker's vowel space be normalised?** Against their own
  measured extremes, or against population norms? Affects whether two people
  produce comparable music or merely internally consistent music.
