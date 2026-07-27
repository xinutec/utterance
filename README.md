# music

Derive music from the structure of a voice.

A recording of someone speaking for half a minute is not treated as a sound to
play back, pitch-shift or resynthesise. It is treated as a **source of law**: the
speaker's prosody, stress hierarchy, vowel geometry and measured harmonic series
are extracted, and the music is what falls out when those constraints are run.

The output is not meant to sound like a voice. Two people reading the same
sentence should produce audibly different pieces, each internally consistent.

## Layout

| Path             | What it is                                                  |
| ---------------- | ----------------------------------------------------------- |
| `music-analysis` | Pure DSP core: audio in, voiceprint out. No IO, no opinions. |
| `src`            | axum server: recordings, analysis runs, static bundle.        |
| `frontend`       | Angular 22 app: capture, inspect, visualise.                  |
| `docs`           | Design intent — start with `docs/architecture.md`.            |

## Running locally

```sh
scripts/dev.sh            # backend on :8181, ng serve on :4200 with /api proxied
```

Then open <http://localhost:4200>.

Backend alone:

```sh
nix develop -c cargo run
```

## Verifying

```sh
scripts/verify.sh         # fmt, clippy, cargo test, eslint, ng build, dev-lint
scripts/setup-hooks.sh    # one-time per clone: pre-commit runs the above
```
