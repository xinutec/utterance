# Test fixtures

## `sustained-vowel.wav`

7.6 s at 44.1 kHz, mono. A real sustained vowel, trimmed from a calibration take
to the phonation plus a little either side.

**What is on it:** *ee → ah → oo*, glided continuously on one unbroken breath,
at a near-constant pitch of about 135 Hz. Confirmed with the speaker. The
phonation fades in over roughly 60 ms, so there is no attack event at the start.

It is here because **no synthetic signal reproduces what it tests.** A generated
tone is perfectly steady; a real held vowel has cycle-to-cycle pitch and
amplitude variation, and slow drift as the tongue settles. That jitter made an
earlier onset detector report 22 events across seven seconds of one continuous
sound, while a synthetic sustained tone passed the same detector cleanly.

**What it can and cannot test.** It bounds how badly onset detection over-fires
on sustained material. It **cannot** establish the right number, because a
continuous glide has no discrete events while still producing real spectral
change; see the module docs in `../../src/onset.rs`. Tests here therefore assert
bounds, never exact counts.

The three vowels land where phonetics puts them — *ee* low-F1/high-F2, *ah*
high-F1/mid-F2, *oo* low on both — which is why the take also serves as a
calibration recording. `tests/speaker_real.rs` asserts only that the profile is
anatomically plausible, since nobody has measured this speaker's corners by any
other means.
