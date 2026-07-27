# Test fixtures

## `sustained-vowel.wav`

7.6 s at 44.1 kHz, mono. A real sustained vowel, trimmed from a calibration take
(recorded 2026-07-27) to the phonation plus a little either side.

It is here because **no synthetic signal reproduces what it tests.** A generated
tone is perfectly steady; a real held vowel has cycle-to-cycle pitch and
amplitude variation, and slow drift as the tongue settles. That jitter is exactly
what made the first onset detector report 22 events across seven seconds of one
continuous sound — and a synthetic sustained tone passed that same detector
cleanly, which is why the bug survived to reach real audio.

**What it must satisfy:** one continuous voiced run, near-constant f0, and one
onset — the attack. See `tests/onset_real.rs`.

No linguistic content: it is a held vowel, not speech.
