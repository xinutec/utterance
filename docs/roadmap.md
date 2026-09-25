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
| noise shape (texture) | done | Centroid, flatness and tilt above 300 Hz. The consonants. |
| stress hierarchy | **not started** | Needed for meter; the continuous mappings sidestep it. |
| phone-class segmentation | **not started** | Needed for the symbol stream. |

All three layers exist and the chain runs end to end from a browser: calibration
takes yield a scale, a timbre palette and a detune; an utterance yields a score;
the score renders to audio on demand at `/api/recordings/{id}/render`, with every
mapping choice reachable as a query parameter and published as a slider.

## The mappings

In rough order of how much each unlocks.

1. **Tuning from measured partials.** *Built* — `utterance-mapping/src/tuning.rs`.
   A Plomp–Levelt roughness curve over the speaker's own measured spectrum,
   swept from unison to the octave, with its deep minima read as scale degrees.
   On real calibration takes:

   - **Reproducible across takes** of the same vowel, to the cent, even where
     individual partials differ by several dB. The curve integrates over every
     pair, so per-partial wobble barely moves it.
   - **Not reproducible across vowels.** One speaker's *ah*, *ee* and *oo* gave
     scales of very different sizes — an open *ah* a nearly-just scale of eight
     degrees, an *ee* the fifth alone. See the open question below.
   - **Audibly not 12-TET.** Several degrees sit well over ten cents off equal
     temperament, and 7:5 at 582 cents has no tempered equivalent.

2. **Harmony from vowel space.** *Built, twice* — `field.rs` and `tonnetz.rs`.
   The field stacks voices at a fixed degree spacing and walks the whole stack
   with the vowel: polyphony from articulation, every moment the same chord at a
   different pitch.

   The Tonnetz maps the two dimensions of vowel space onto a harmonic lattice
   spanned by two of the speaker's own consonances, and quantises position on it
   to a triangle:

   - **Chords hold.** While the mouth stays in one triangle the pitches do not
     move, so a sustained vowel is a sustained chord and the derived tuning has
     something to be audible *in*. Every other stream keeps moving underneath.
   - **Voice leading falls out of the geometry.** Neighbouring triangles share
     two of three pitches, and a kept pitch keeps its frequency, so a chord
     change holds two voices and steps one. Nobody wrote that rule.
   - `hold` (hysteresis in space) and `settle` (a minimum dwell in time) decide
     how readily the harmony follows the mouth — together, whether a chord rings.

   **Heard against each other by both listeners: neither wins.** Both "sound a
   bit like music", and both stay (see *Freedom is a feature*). That settles the
   question the Tonnetz was built to ask — a held harmony is not better than a
   sliding one by enough to prefer it — and redirects effort to what both lack:
   nothing operates above the phrase, and there is no meter.

3. **Meter from stress hierarchy.** Nested strong/weak grouping from syllable
   prominence. Blocked on stress measurement, but off the critical path: the
   continuous mappings need no rhythm at all.

4. **Development from the symbol stream.** Phone classes as an alphabet for a
   deterministic rewrite system. The furthest out and least specified: structure
   at every timescale at once.

## Known gaps, with their cost

- **Onsets mean "the spectrum changed", not "a syllable began."** Spectral flux
  cannot separate the two; a continuously glided vowel demonstrates it. Tuning
  the threshold needs ground truth on speech, so the onset tests assert bounds
  rather than counts. The real fix is the stress hierarchy, which carries a cue
  flux does not. The `notes` mapping inherits this: its rhythm is wrong, and it
  is kept because comparing mappings is how any of them get judged.

- **Ground truth for syllables should come from the text, not the audio.**
  Marking syllable onsets by ear was built (`/label`, recoverable at `5ff52dd`)
  and removed: placing an onset precisely is slow, skilled work, far more than
  the ask it was presented as. The measurement wanted is a *count*, and a known
  passage has a known number of syllables — the detector either reports it or
  does not, and counting syllables in writing is a desk task. Isolated letters
  only establish a floor; one read sentence with a hand-counted total measures
  connected speech. Asking anyone to record it is parked.

- **No formant continuity tracking.** Assignment is per-frame, constrained by
  anatomical range; a formant that drops out is nulled rather than filled from
  the one above — correct, but lossy.

  A Viterbi pass scoring whole assignments across frames was written, measured
  and reverted. On the real fixture it filled *fewer* slots and moved F2 up into
  what the per-frame rule calls F3; raising the empty-slot cost plateaued. It
  passed on synthetic vowels only because a synthesised vowel does not move, so
  continuity always wins — **the property was checked against signals that could
  not falsify it.** The blocker is ground truth: the only available score is
  *how many slots got filled*, and optimising that proxy would undo the reason
  the per-frame rule exists.

- **Analysis is not cheap.** Measuring partials runs a 2048-point FFT on every
  steady frame, so a take costs seconds rather than milliseconds. Fine once per
  recording, but a re-analysis sweep after a schema bump is something you wait
  for.

- **Speech does not hold a chord.** `src/bin/dwell.rs` measures how long each
  Tonnetz chord rings. Sustained and sung takes spend most of their time in
  rings of a second or more — long enough for a tuning to be perceptible — while
  speech rings in fractions of a second at any `hold`: it does not hold a vowel
  long enough for spatial hysteresis to help. `settle` removes the flickers
  `hold` cannot (a chord held for seconds that flicks to a neighbour for two
  frames and back) without freezing anything; past a point on speech it lags
  enough to chase the mouth and makes new short rings, which is why it is a
  published range. It defaults to 0, reproducing the behaviour before it
  existed; where it should sit is a question for the ear.

- **The Tonnetz says nothing about register.** Each voice takes whichever octave
  of its pitch class falls nearest a target, which keeps common tones at common
  frequencies. It does nothing a voice-leading rule would recognise — no contrary
  motion, no avoidance of parallels, no bass line. Whether any of that is wanted
  is a taste question.

- **A scale of the fourth and the fifth spans no lattice.** `density` high enough
  prunes a real scale to the tonic, the fifth and the octave: one direction, no
  plane. `Lattice::from_tuning` then fails with a reason, the render answers 422
  `unplayable` naming the intervals and the knob that undoes it, and the voice
  summary carries the same verdict so the page can say so before a player is
  pointed anywhere. Where the threshold falls is a fact about the speaker, so
  nothing clamps the slider.

- **Not every stream is read.** `src/bin/streams.rs` correlates every stream a
  mapping reads, over the sounding frames of every take. Tilt earns admission —
  it moves where brightness does not — but no mapping reads it yet; what it
  should *drive* is a question for the ear. Flatness tracks aperiodicity closely
  enough to add nothing and stays unread. Harmonic-to-noise per band is not
  measured: its distinct claim is that periodicity varies *across* bands (a
  breathy voice is periodic low and noisy high), and that is testable with the
  same tool before any of it is built.

- **Nothing operates above the phrase.** The field moves at three timescales —
  level, articulation, prosodic drift — and the longest is two seconds. A piece
  has a shape across its whole length and nothing here produces one.

  **Recurrence was the unblocked route to a harmonic plan, and the probe refused
  it** (`src/bin/form.rs`). Every other route ends behind syllables somebody
  marks by ear; resemblance needs none. Measured over the store — a
  self-similarity matrix over the eight mapped streams, Foote novelty from 1 to
  16 seconds, and the rate at which distant frames resemble each other — real
  takes do not beat a phase-randomised surrogate of themselves at any scale.

  The control is what makes that a result. A block shuffle manufactures a
  boundary at every block edge, so its bias runs along the very axis measured;
  a raw peak novelty is a contest the noisier curve wins. The surrogate that
  survives keeps each stream's Fourier magnitudes and replaces the phases, one
  phase sequence across all eight, so it has the take's smoothness, drift and
  correlations and nothing of its arrangement.

  **The limit:** a phase surrogate keeps every periodicity, so a genuinely
  periodic form is structure it shares. This measures that the structure is not
  strong enough to build a mapping on blind — not that there is none — and says
  nothing about material performed with sections in mind, which the store does
  not hold.

## Decisions taken

Recorded so they are not reopened without reason. **This is not the full
ledger** — most decisions are documented at the code they govern. Listed here
are the ones with no obvious home in the source, plus the ones that most shape
daily work.

- **A stored voiceprint is a cache, not a record.** The audio is the source of
  truth and analysis is a pure function of it, so `SCHEMA_VERSION` identifies the
  analyser and `Store::ensure_current` re-derives anything stale. Bump it for
  *any* change to the output, algorithm as much as shape: a shape change fails
  loudly on deserialise, an algorithm change is silent.

- **Capture stays in the browser.** Server-side capture — the host recording
  from its own microphone with a phone as remote — was rejected: it doubles the
  capture paths to maintain. Browsers allow the microphone only in a secure
  context, so recording works on `localhost` and on the deployed HTTPS site, not
  on a plain-HTTP LAN address.

- **No ML.** See `architecture.md`. The mapping layer needs the derivation, not
  an inferred number.

- **Aesthetic parameters live in the mapping layer.** A knob in analysis would
  invalidate every stored voiceprint each time it moved; a knob in mapping is
  swept against a fixed voiceprint and heard immediately.

- **A take says what it is for, and only calibration takes define the
  speaker.** `Role::Calibration` or `Role::Material`, declared at upload or
  later through `PUT /api/recordings/{id}/role`, defaulting to material. The
  store holds other people's singing to render; pooled into the profile it would
  describe an anatomy belonging to nobody. This is role, not ownership — there
  is one user.

  - **One take per calibration step, most recent wins.** A step is re-recorded
    because the earlier take was bad; averaging would keep it counting.
  - **The role survives re-analysis.** It is not in the audio, so
    `ensure_current` carries it across; defaulting it would dissolve the speaker
    on the next schema bump.
  - **A store with no calibration take refuses to render** and says to record
    the guided vowels.
  - **A vowel's identity comes from the prompt, not the audio.** A take carries
    its calibration step's id as its label, so the take recorded against *ee* is
    this speaker's *ee* by construction and nobody marks anything by ear. The ids
    are one enum in Rust (`CalibrationStep`), exported to TypeScript. `steady-ah`
    does not place the open corner: it is recorded for its spectrum, and its
    length would let a more casual *ah* win the corner by weight of evidence.

- **Taxonomies are enums, and the browser reads the same union.** `Mapping`,
  `Material`, `KnobName`, `ErrorCode`, `CalibrationStep` and `Role` are Rust
  enums exported by ts-rs, so a name the backend does not serve cannot be written
  in the frontend and a rename is a build error on both sides. Dispatch goes
  through `Mapping::score_with`, whose exhaustive match means a new mapping does
  not compile until it has a score. Where a `&'static str` must restate a serde
  spelling, a test round-trips it.

- **A knob is declared once.** `knobs!` in `utterance-mapping/src/params.rs`
  generates the `Knob` table, the `Params` field, `Default`, `sane`, `with`,
  `KnobName` and the `KnobQuery` the route deserialises. `params::Knob` is also
  the wire type: the API forwards it unchanged, so a copy would only restate it.

- **A vowel space is normalised against the speaker's own extremes**, not
  population norms. It makes the *utterance* the variable rather than the
  anatomy, so a body of work by one person is one sound world with a different
  piece in each take. The cost is a calibration per speaker.

- **Anything a mapping normalises against is measured per speaker.** Pitch
  range, vowel space, F3 range and brightness all live in `SpeakerProfile`.
  Against a constant, a measurement stops meaning *bright for them*; against the
  take, the difference between two things one person said is normalised away —
  which is why the field's pitch drift is relative to the profile's tonic, not
  the take's median.

- **Control is exercised by learning the mapping, not by playing it live.**
  Real-time would force analysis to become causal and give up non-causal pitch
  tracking and whole-take statistics, before anyone knows whether the mapping is
  worth performing. Deterministic mapping plus fast iteration gets most of the
  control: record, hear it, adjust, sing it again. Determinism is also the
  strongest argument for no ML — a singer can only build a mental model of a
  system that answers the same way twice.

  **Constraint kept while this holds:** mappings stay frame-local where they
  can, so real-time remains reachable. The speaker profile is measured once per
  person, not per take, so it is not a violation.

- **Freedom is a feature** (Pippijn, on hearing the Lattice against the field and
  liking both). Keeping alternatives is worth something in itself: a mapping is
  not on probation waiting to be beaten. The same instinct runs through the code
  — nothing clamps `density` where the lattice refuses, anything arguable is a
  parameter, a published range is a promise. The person listening goes where
  they want, and the software says what happened rather than preventing it.

  **What it rules out:** converging on one blessed mapping, removing a knob
  because the default beats its ends, and narrowing a range to the part that
  currently sounds good.

- **Mappings are alternatives, not a pipeline.** `notes` emits events and no
  field; `field` and `tonnetz` emit a field and no notes; all carry the
  consonants. A score carries one field and one list of events, so two mappings
  making the same material are refused together rather than one silently
  winning. What each makes is published, so the browser turns a rival off rather
  than assembling a combination the route rejects.

- **Anything arguable is a parameter, not a constant.** If a value could
  reasonably be chosen differently it belongs in `params::Params` and is
  reachable from the render URL. A new knob's default reproduces the behaviour
  from before it existed, so renders stay comparable.

- **A knob says which mappings it reaches, and whether it is primary.** A slider
  shown while a mapping that ignores it plays moves and changes nothing, so the
  knob declares its mappings and `tests/api.rs` renders each against every
  mapping it claims, failing if the audio is unchanged. `Knob::primary` splits
  the controls that decide what kind of piece this is from those that adjust a
  piece you have: an instrument panel of equal-weight sliders is a set of things
  to get wrong, and a panel nobody dares touch costs evidence. Primary is not a
  ranking by audible authority — `bind` is primary because it is the axis the
  project argues about, `spacing` for how much it changes. `reach` was moved
  out by ear: a claim about what matters, made by the person listening, beats a
  claim about what *ought* to matter.

- **A published range is a promise about every position on it.** Every value a
  slider can reach either makes a sound or refuses and says which setting to
  move — never a valid 200 carrying nothing. Checked over whole ranges by
  `every_setting_a_slider_can_reach_either_sounds_or_says_why_not`. Clamping the
  range instead was rejected: where the lattice gives out moves per speaker, and
  a slider that silently stopped somewhere different for everyone would be a
  worse lie than the refusal.

- **The mapping publishes its own controls.** `GET /api/controls` serves the
  knob table and the mappings, and the UI builds its sliders from that, so a knob
  added in Rust appears with no frontend change and a range cannot drift.

- **A comparison is a link.** `/compare` plays two settings at once with one
  muted, so switching is instant and at the same moment of the piece, and draws
  each stream's difference scaled to its own largest gap. The page reads `take`,
  `a` and `b` from its URL and writes its state back, each side a whole settings
  query encoded inside the outer one. A comparison is this project's unit of
  evidence and there are two listeners in two places: passed on as instructions,
  they hear two slightly different things and disagree about a result neither
  heard. A URL is input from outside, so an unpublished knob is dropped and an
  out-of-range value clamped.

- **One stream drives one parameter.** What a listener hears as variety is how
  many things can move *independently*; a mapping that quietly reads one stream
  into two parameters sounds simpler than the voice it came from.
  `src/bin/streams.rs` checks it.

- **A lattice is judged by its worst interval.** The roughness curve measures
  each degree against the tonic only, so the third interval of a triangle — the
  difference of the two axes — was never measured. A pair is admitted only if
  that difference is also a consonance, and the chosen pair is the one whose
  *shallowest* minimum is deepest. On the voice measured so far this yields a
  fifth and a minor third, whose difference is the just major third: the
  classical Tonnetz, derived rather than assumed. Where no pair qualifies it
  falls back to the deepest independent pair.

- **`bind` binds the note, not the lattice.** A lattice point's pitch is
  `x·a + y·b`, so binding the axes would move far-out chords by more than a
  quarter tone and eventually into different notes — two settings of a
  comparison would be two different pieces. The lattice is laid out on the
  speaker's own scale unconditionally and `bind` is applied to each sounding
  pitch.

- **The bundle budget describes this app, not a public one.** The
  initial-bundle warning is 800 kB rather than `ng new`'s 500: this is served to
  two people, not to strangers on a mobile connection. The error ceiling is
  untouched.

## Open questions

- **Is the derived tuning audible?** `bind` interpolates each degree between the
  speaker's scale (1) and equal temperament (0). On the field mapping the two are
  almost indistinguishable by ear. `src/bin/beating.rs` shows why: at 1 the
  strongest partial coincidences in a chord beat at a fraction of a hertz, at 0 at
  5–14 Hz — a real physical difference that needs roughly a second of stable
  chord to hear, and the field never holds still that long.

  The Tonnetz holds chords, and `dwell` shows sustained and sung takes ringing
  long enough while speech does not. So the question is narrower: **is it audible
  on sung material?** The honest listening test is a pair — one sung take where
  the numbers say yes, one spoken take where they say no. Hearing a difference on
  both would mean the difference is not the tuning. On the Tonnetz a `bind`
  change can occasionally send a voice across an octave boundary, which is more
  noticeable than the tuning; compare in the middle of a held chord and discount
  the leaps.

- **Which knobs change what anyone hears?** `src/bin/authority.rs` measures each
  knob on each continuous mapping along several axes — pitch (maximum *and*
  typical, since a knob that mostly nudges and occasionally re-registers a voice
  does two things), chord roughness, loudness balance, colour, ring duration and
  consonant level — and deliberately does not sum them: weights would be a claim
  about what matters, which is what the listening is meant to settle. Every axis
  is there because its absence produced a false zero.

- **Should a listener be able to perceive the connection back to the voice?**
  The largest single constraint on the mapping layer: a perceptible link keeps
  mappings legible, a private seed frees them to be arbitrarily abstract. The
  lean is perceptible, because it is what makes the project legible to anyone but
  its authors. This is *not* the same axis as the next, and the two may run
  opposite: binding hard to the speaker yields the most derived and least
  speech-like music, while a pitch contour quantised to an ordinary scale is
  obviously melodised speech that transforms almost nothing.

- **How far should the voice be allowed to bind?** The axis is convention ↔
  speaker in every dimension: interpolate between the derived and the tempered
  degree, between a measured tempo ratio and a small-integer one, between the
  vowels' lattice path and the nearest diatonic waypoint. One scalar per
  dimension, not one global scalar — binding tuning hard while leaving rhythm
  conventional is a different, probably more listenable, result. `bind` is that
  control for tuning; the others wait on meter.

- **Which vowel does a speaker's tuning come from?** A harmonic series belongs to
  a tract shape, so "the speaker's scale" is undefined until the vowel is pinned
  down. Candidates: a single nominated calibration vowel, the union of several,
  or a scale that changes with the vowel being sung — the most interesting and
  the most likely to be unusable. `tuning::MIN_DEPTH` (the `density` knob) decides
  when a dip counts as a note, and part of the spread between vowels is that
  threshold rather than the voice.

  Until it is settled, `src/voice.rs` picks the calibration take yielding the
  richest scale and `?calibration=<id>` overrides it. The obvious criterion —
  most steady frames — picks a long *ee* whose scale is the fifth alone over a
  shorter *ah* with eight degrees: measurement quality and musical usefulness
  point in opposite directions, which is itself an argument that this has to be
  answered deliberately.
