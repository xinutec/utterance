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

**What is actually on it:** *ee → ah → oo*, glided continuously on one unbroken
breath, at a near-constant pitch of about 135 Hz. Confirmed with the speaker.

**What it can and cannot test.** It bounds how badly onset detection over-fires
on sustained material — the first implementation reported 22 events across seven
seconds of one continuous sound. It **cannot** establish the right number, because
a continuous glide has no discrete events while still producing real spectral
change; see the module docs in `../../src/onset.rs`. Tests here therefore assert
bounds, never exact counts.

Note also that the phonation *fades in* over roughly 60 ms rather than starting
sharply, so there is no attack event to detect at the beginning.

No linguistic content: it is a held vowel, not speech.
