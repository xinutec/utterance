# utterance

Derive music from the structure of a voice.

A recording of someone speaking for half a minute is not treated as a sound to
play back, pitch-shift or resynthesise. It is treated as a **source of law**. The
aim: the speaker's prosody, stress hierarchy, vowel geometry and measured
harmonic series become the constraints, and the music is what falls out when
those constraints are run.

The output is not meant to sound like a voice. Two people reading the same
sentence should produce audibly different pieces, each internally consistent.

The chain runs end to end: guided calibration takes yield a scale and a timbre
derived from the speaker's own spectrum; an utterance in that world yields a
score; the score renders to audio. The browser records takes, shows each
voiceprint, plays the derived music with every mapping choice on a slider, and
compares two settings side by side. `docs/roadmap.md` says what is next and
which decisions are settled.

## Layout

| Path                    | What it is                                                     |
| ----------------------- | -------------------------------------------------------------- |
| `utterance-analysis`    | Pure DSP core: audio in, voiceprint out. No IO, no opinions.   |
| `utterance-mapping`     | Musical decisions over a voiceprint. Where the opinions live.  |
| `utterance-realisation` | Score to audio. Additive synthesis, no decisions.              |
| `src`                   | axum server: recordings, analysis, renders, the static bundle. |
| `src/bin`               | Research tools that measure what the mappings do.              |
| `frontend`              | Angular app: capture, calibrate, inspect, listen, compare.     |
| `android`               | A WebView wrapper with microphone access.                      |
| `docs`                  | Design intent. Start with `architecture.md`, then `roadmap.md`. |

## Running locally

While working on the code — live reload, backend and frontend separate:

```sh
scripts/dev.sh            # backend on :8181, ng serve on :4200 with /api proxied
```

Then open <http://localhost:4200>.

To just *use* it, serving the built bundle and the API from one origin:

```sh
nix develop -c bash -c 'cd frontend && pnpm install --frozen-lockfile && pnpm run build'
BIND_ADDR=0.0.0.0:8181 \
  DATA_DIR="$PWD/data" \
  STATIC_DIR="$PWD/frontend/dist/utterance-web/browser" \
  nix develop -c cargo run --release
```

Then <http://localhost:8181>. Binding `0.0.0.0` also serves the LAN, so another
device can browse takes and inspect voiceprints — but **not record**: browsers
only grant microphone access in a secure context, which `localhost` is and a
plain-HTTP LAN address is not.

Backend alone, API only:

```sh
nix develop -c cargo run
```

Recordings live in `data/`, one directory per take holding the audio, its
voiceprint and a little metadata. Deleting `data/` is a supported way to start
over; the audio is the only thing not recoverable, since voiceprints are
re-derived automatically whenever the analyser moves on.

## Verifying

```sh
nix run ../dev-lint#gate -- . gate.json   # every check in gate.dhall
scripts/setup-hooks.sh                    # one-time per clone: pre-commit runs the gate
```
