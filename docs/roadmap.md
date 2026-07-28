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
| speaker profile | done | Vowel-space corners, F3 range, f0 range, brightness range. |
| measured partial ratios | done | Per take, over frames steady enough to use. |
| noise shape (texture) | done | Centroid and flatness above 300 Hz. The consonants. |
| stress hierarchy | **not started** | Was needed to fix onsets; the field mapping sidesteps it. |
| phone-class segmentation | **not started** | Needed for the symbol stream. |

All three layers exist and the chain runs end to end from a browser: a
calibration take yields a scale, a timbre palette and a detune; an utterance
yields a score; the score renders to audio on demand at
`/api/recordings/{id}/render`, with every mapping choice reachable as a query
parameter.

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

2. **Harmony from vowel space.** *Built, twice* — `field.rs` and `tonnetz.rs`.
   The first stacks five voices at a fixed degree spacing and walks the whole
   stack with the vowel: polyphony from articulation, and every moment the same
   chord at a different pitch.

   The second is the Tonnetz. The two dimensions of vowel space become the two
   dimensions of a harmonic lattice, spanned by the two deepest independent
   minima of the speaker's own roughness curve — for a voice, near the fifth and
   a third, which is a result rather than an assumption. Position on it is
   quantised to a triangle, and that is the part that matters:

   - **Chords hold.** While the mouth stays in one triangle the pitches do not
     move, so a sustained vowel is a sustained chord and there is finally
     something for the derived tuning to be audible *in*. Everything else the
     voice does goes on moving underneath it.
   - **Voice leading falls out of the geometry.** Triangles sharing an edge
     share two of their three pitches, and a pitch the lattice keeps keeps its
     frequency, so a chord change holds two voices and steps one. Nobody wrote
     that rule.
   - `hold` is the knob that decides how far past a boundary the mouth must go
     before the harmony follows — the one that decides whether a chord rings.

   **Unheard as of 2026-07-28.** Built, tested, and not yet listened to.
3. **Meter from stress hierarchy.** Nested strong/weak grouping from syllable
   prominence. Still blocked on stress measurement — but no longer on the
   critical path, because the field mapping needs no rhythm at all. It went from
   the thing everything waited on to an enrichment.
4. **Development from the symbol stream.** Phone classes as an alphabet for a
   deterministic rewrite system. The furthest out and the least specified, and
   the place the project's oldest idea still lives: structure at every timescale
   at once.

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
- **Nothing sustained, so the tuning could not be heard.** The derived scale is
  real and was inaudible, because a chord has to ring for about a second before
  its tuning is perceptible and the field mapping never held still that long.
  The Tonnetz mapping is the answer built for it — quantising the harmony while
  leaving the time continuous — and whether it is *enough* is a listening
  question nobody has answered yet.
- **The Tonnetz says nothing about register.** Each voice takes whichever octave
  of its pitch class falls nearest a target, which keeps common tones at common
  frequencies and is why voice leading survives. What it does not do is anything
  a voice-leading rule would recognise: no contrary motion, no avoidance of
  parallels, no bass line. Whether any of that is wanted is a taste question and
  should be settled by ear.
- **A scale of the fourth and the fifth spans no lattice**, and the honest
  response is that the Tonnetz mapping produces nothing at all for that speaker.
  *Hit in listening, 2026-07-28, and now said out loud.* It was not a thin
  calibration that got there but the `density` knob: past about 0.14 on a real
  take the scale prunes to the tonic, the fifth and the octave, which is one
  direction and no plane. The render refused nothing and returned a score with
  no field in it, so the answer was a valid 200 containing consonants over
  silence — indistinguishable from a broken build.

  `Lattice::from_tuning` now fails with a reason rather than an absence; the
  route turns that into a 422 `unplayable` naming the intervals it had and the
  knob that undoes it; and the voice summary carries the same verdict, because
  it is fetched by script before the player is pointed anywhere and an `<audio>`
  element handed a failing URL shows a broken control and no words.

  **Where the threshold is, is a fact about the speaker**, not a constant — it is
  wherever their second-deepest minimum falls. Nothing clamps the slider for
  that reason: a limit that moves per person, silently, would be a worse lie
  than the refusal.

- **Comparing is now a page rather than an exercise in URLs.** `/compare` plays
  two settings at once with one muted, so switching is instant and at the same
  moment of the piece, and draws each stream's difference scaled to its own
  largest gap. Built after four separate attempts to answer the `bind` question
  by ear failed for want of an instrument.
- **The API's voice fixture was less of a voice than any voice** (2026-07-28,
  fixed). Two formants and a textbook source slope gave a four-degree scale
  whose two deepest intervals were the fourth and the fifth — the one pair that
  spans no harmonic lattice — where a real take through the same code gives
  eight. Every mapping that reads the *shape* of a scale was being tested
  against something shaped like nothing. Now three formants and a shallower
  source, tuned until the measured partials resemble a measured voice's.
- **The field reads eight streams; the voice emits about ten.** What is still
  unread is the *shape* of the spectrum beyond its centroid — a tilt measurement
  proper, and the harmonic-to-noise balance per band. Neither is measured yet, so
  this is analysis work rather than mapping work.
- **Nothing operates above the phrase.** The field moves at three timescales —
  level, articulation, prosodic drift — and the longest is two seconds. A piece
  has a shape across its whole length and nothing here produces one. The Tonnetz
  buys time at the chord's timescale and no more; it is a held harmony, not a
  harmonic plan.
- **The note mapping's rhythm is still wrong.** `compose.rs` reads onsets, which
  mean *the spectrum changed* rather than *a syllable began*. It is kept because
  comparing mappings is how any of them get judged, not because it is right.

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

- **Mappings are alternatives, not a pipeline** (2026-07-28). `compose` emits
  notes and no field; `field` and `tonnetz` each emit a field and no notes; all
  of them carry the consonants, because a consonant is a thing that happens at a
  moment whichever way the pitched material is made. The reason to keep the
  weaker ones is that comparison is the only way any of them gets judged.

  **A score carries one field and one list of events**, so two mappings making
  the same material are refused together rather than one of them silently
  winning — whichever lost would be a mapping someone asked for and did not
  hear. Which mappings compete is a column of the table in `routes/api.rs` and
  is published to the UI, so the browser turns one off rather than letting
  someone assemble a combination the route rejects.

- **A knob says which mappings it reaches** (2026-07-28). `hold` belongs to the
  Tonnetz and `voices` to neither of the note mappings, and a slider shown while
  a mapping that ignores it is playing is the same failure the knob table exists
  to prevent — it moves, and nothing changes, and the person concludes the thing
  is broken. Declared on the knob, published, and checked: `tests/api.rs` renders
  every knob against every mapping it claims and fails if the audio is unchanged.

- **Anything arguable is a parameter, not a constant** (2026-07-28). If a value
  could reasonably be chosen differently it belongs in `params::Params` and is
  reachable from the render URL. A constant can only be changed by editing,
  rebuilding and re-rendering, which is the wrong loop for decisions that are
  settled by ear. Defaults reproduce the unparameterised behaviour, so old
  renders stay comparable with new ones.

- **A published range is a promise about every position on it** (2026-07-28).
  A knob declares `min`, `max` and `step`, and a slider will be dragged to both
  stops — so every value it can produce has to either make a sound or refuse and
  say which setting to move. There is deliberately no third answer, because the
  third answer is what `density` did to the lattice: a valid 200 carrying no
  field, heard as nothing and reported as success.

  Checked over the whole range rather than at one sample, by
  `every_setting_a_slider_can_reach_either_sounds_or_says_why_not` in
  `tests/api.rs`. The ends are the interesting part: they are what the range
  promises and the part nobody drags to by hand. The neighbouring test sweeps one
  value per knob and asks a different question — whether the knob does anything
  at all.

  **The alternative, clamping the range, was rejected.** Where the lattice gives
  out depends on where a speaker's second-deepest minimum falls, so the limit
  moves per person; a slider that silently stopped somewhere different for
  everyone would be a worse lie than the refusal.

- **The bundle budget describes this app, not a public one** (2026-07-28). The
  initial-bundle warning was raised from 500 kB to 800 kB when the knobs brought
  Material's slider, select and form-field in — about 230 kB uncompressed. The
  500 was the figure `ng new` writes for a site served to strangers over a
  mobile connection; this one is served from a Mac on a LAN to two people, so
  the old number measured nothing anyone cares about while making a real warning
  easy to miss. The error ceiling is untouched.

- **One stream drives one parameter** (2026-07-28). Found by counting rather
  than by listening: the field's doc claimed six streams while `colour` was set
  from the same normalised F2 that walks the root, so the timbre could only
  change when the harmony did. Two streams welded into one. What a listener
  hears as variety is how many things can move *independently*, so the count is
  only honest if each stream reaches something of its own — and a mapping that
  quietly doubles up will always sound simpler than the voice it came from. The
  field now reads eight: f0, frontness, openness, F3, flux, energy, centroid,
  aperiodicity.

- **Anything a mapping normalises against is measured per speaker**
  (2026-07-28). Brightness and F3 join the vowel space and the pitch range in
  `SpeakerProfile`. The alternative — a fixed range, or the take's own — fails
  the same way each time: against a constant it stops meaning *bright for them*,
  and against the take it normalises away the difference between two things the
  same person said. This is the third time that reasoning has decided a design
  question, which is why it is written here as a rule rather than a third time
  as a case.

- **The mapping publishes its own controls** (2026-07-28). `GET /api/controls`
  serves `music_mapping::params::KNOBS` — each knob's range, step, starting
  value and one line saying what it does — and the UI builds its sliders from
  that rather than from a list of its own. A knob added to the table appears in
  the browser with no frontend change, and a range that moves cannot leave a
  slider offering values the mapping clamps away. The failure this prevents is
  specific and hard to spot: a control that appears to work, moves, and changes
  nothing. `tests/api.rs` closes the loop by driving every published knob
  through a real render and failing if the audio is unchanged.

- **Prosody is measured against the speaker, not the take** (2026-07-28). The
  field's pitch drift is relative to the profile's tonic rather than to the
  utterance's own median. Against the take's median, an utterance spoken entirely
  higher produces identical music — the thing that makes one reading different
  from another is normalised away. Caught by a test, not by reasoning.

## Open questions

- **Where on the convention-to-speaker axis is the music?** Swept, listened to,
  and **not answerable as posed** (2026-07-28). Against every take in the store,
  `bind=0` and `bind=1` are almost indistinguishable by ear.

  Why, measured rather than guessed. The whole effect of `bind` is whether
  partials of different voices land on each other or near each other: at 1 the
  five strongest coincidences in the chord beat at 0.01–0.26 Hz — locked — and
  at 0 the same ones beat at 4.8–14.3 Hz. Total chord roughness differs by 4%.
  That is a real physical difference and it is inaudible here, because a beat
  of a few hertz needs roughly a second of stable chord before anyone hears it
  and the field never holds still that long: across a take the voices travel
  3–4 semitones on the steadiest sustained vowel and 12–25 on speech.

  **So the project's central claim is currently unhearable rather than wrong.**
  The scale is derived, measured, and reproducible; the mapping moves too fast
  to expose it. Answering this question needs a mapping whose harmonic rhythm is
  slow enough for a chord to ring — which is the same thing the "nothing
  operates above the phrase" gap asks for, from the other end.

  **Acted on, 2026-07-28.** The trade looked like sustain *or* continuous
  tracking, and it is not one: what has to hold still is the harmony, not the
  music. The Tonnetz mapping quantises where the vowel sits on a harmonic
  lattice and leaves every other stream — loudness, colour, breath, drift —
  moving at its own rate, so a held vowel gives a held chord without a frame
  going unread. Whether the chords now ring long enough for `bind` to be
  audible is the listening test this whole page has been waiting for, and it has
  not been done.

- **Which knobs actually change what anyone hears?** Measured on `vowel-ah` in
  the field mapping, as how far each moves the pitch at its widest: `density`
  1516 cents, `spacing` 1200, `voicing` 632, `reach` 600, `drift` 53, `bind` 18.
  Worth keeping in view when choosing what to build: the knobs that restructure
  the chord dominate the ones that adjust it. Not re-measured for the Tonnetz,
  where the same knobs act on a different geometry and `bind` has a chord that
  holds still to be heard in.

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
