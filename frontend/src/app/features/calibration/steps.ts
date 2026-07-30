/**
 * The guided calibration: what to say, in what order, and what makes a take usable.
 *
 * One recording per step, self-advanced. Deliberately *not* prompts appearing
 * inside a single long take: a prompt time is not a label. People lag a prompt,
 * drift and start early, so slicing one recording by when the screen changed
 * would mean measuring the UI's timing rather than the voice's — and the
 * measurements would look exact while being wrong. A take per step makes the
 * label free and exact.
 *
 * The checks here judge a take against what its step was for. They never block:
 * a person can keep any take, because the notes below are heuristics about
 * usable material and the person is the one who heard the room.
 */

import type { CalibrationStep as StepId, RecordingDetail } from "../../models";

/** What a take must look like to serve its step. */
interface Requirements {
  /** Below this the step did not really happen. */
  readonly minS: number;
  /** Fraction of frames that must carry a fundamental, where that is the point. */
  readonly minVoiced?: number;
  /**
   * How far f0 may wander, in semitones, between its 5th and 95th percentile.
   *
   * Only set where holding a pitch still is the point of the step. Absent means
   * unchecked rather than unlimited — the pitch-range steps are sustained too,
   * but there the note being reached matters and a wobble does not.
   */
  readonly maxDriftSemitones?: number;
}

/** One thing to record. */
export interface CalibrationStep {
  /**
   * Stable id, used verbatim as the take's label so the audio is self-describing.
   *
   * Typed against the backend's own list (`src/calibration.rs`, generated here
   * by ts-rs) rather than as a string. The backend reads a take's label to know
   * which vowel it is — that is where the speaker's own vowel corners come from
   * — so a rename on either side would quietly stop a take being a vowel, with
   * nothing failing and no measurement missing enough to notice. Now it does not
   * compile.
   */
  readonly id: StepId;
  readonly title: string;
  /** The imperative, shown large. Everything a person needs if they read nothing else. */
  readonly instruction: string;
  /** The rest, as short lines. */
  readonly detail: readonly string[];
  /** What this take is measured for — shown so the step is not a ritual. */
  readonly purpose: string;
  /** Suggested length. Not enforced; `requirements.minS` is what is checked. */
  readonly targetS: number;
  readonly requirements: Requirements;
}

export const STEPS: readonly CalibrationStep[] = [
  {
    id: "steady-ah",
    title: "A steady note",
    instruction: 'Hold "ah" for about ten seconds, as steady as you can.',
    detail: [
      "As in father. Breathe in first.",
      "One comfortable pitch — no vibrato, no swell, no drift.",
      "Keep your mouth and tongue frozen in place.",
      "Stop before you run out of breath; the last second goes unstable.",
      "Worth doing two or three times. Steadiness matters more than length.",
    ],
    purpose:
      "The tuning system is derived from this one. Anything moving in your mouth moves the spectrum being measured.",
    targetS: 10,
    requirements: { minS: 4, minVoiced: 0.5, maxDriftSemitones: 1 },
  },
  {
    id: "vowel-ee",
    title: "Corner one — ee",
    instruction: 'Hold "ee" for two or three seconds.',
    detail: [
      "As in feet.",
      "Held, not glided into anything else.",
      "Same pitch as the steady note.",
      "Slightly exaggerated is good — reach the extreme of the mouth shape.",
    ],
    purpose: "One corner of your vowel space. Harmony is mapped from the shape those corners make.",
    targetS: 3,
    requirements: { minS: 1.5, minVoiced: 0.4, maxDriftSemitones: 1.5 },
  },
  {
    id: "vowel-ah",
    title: "Corner two — ah",
    instruction: 'Hold "ah" for two or three seconds.',
    detail: ["As in father.", "Held, same pitch as before.", "Jaw open — this is the far end from ee."],
    purpose: "The open corner: your highest first formant.",
    targetS: 3,
    requirements: { minS: 1.5, minVoiced: 0.4, maxDriftSemitones: 1.5 },
  },
  {
    id: "vowel-oo",
    title: "Corner three — oo",
    instruction: 'Hold "oo" for two or three seconds.',
    detail: ["As in boot.", "Held, same pitch as before.", "Lips rounded and forward."],
    purpose: "The back corner: both formants low. With ee and ah this closes the triangle.",
    targetS: 3,
    requirements: { minS: 1.5, minVoiced: 0.4, maxDriftSemitones: 1.5 },
  },
  {
    id: "pitch-low",
    title: "Your lowest note",
    instruction: 'Sing "ah" on the lowest note you can hold comfortably.',
    detail: [
      "Comfortable, not strained.",
      "Not creaky — if it rattles rather than rings, go up a little.",
      "Hold it about three seconds.",
    ],
    purpose: "The bottom of your range. Sets where the derived music can put its foundation.",
    targetS: 3,
    requirements: { minS: 1.5, minVoiced: 0.4 },
  },
  {
    id: "pitch-high",
    title: "Your highest note",
    instruction: 'Sing "ah" on the highest note you can hold in full voice.',
    detail: [
      "Full voice, not falsetto.",
      "Falsetto is a different mechanism with a much weaker harmonic series — it would describe a different instrument.",
      "Comfortable, not strained. Hold it about three seconds.",
    ],
    purpose: "The top of your range, in the same vocal mechanism as everything else here.",
    targetS: 3,
    requirements: { minS: 1.5, minVoiced: 0.4 },
  },
  {
    id: "speech",
    title: "Talk normally",
    instruction: "Talk about anything for about a minute. Don't read.",
    detail: [
      "Something you can talk about without composing it — what you did yesterday, how something at work actually works, an argument you're still annoyed about.",
      "Natural pace. Pauses are fine and useful.",
      "Don't read anything aloud: read speech has flatter pitch movement and more regular pausing than talking does, and your prosody is one of the things being measured.",
    ],
    purpose: "Your prosody, your stress, and where your vowels actually land in ordinary speech rather than deliberate ones.",
    targetS: 45,
    requirements: { minS: 15, minVoiced: 0.15 },
  },
];

/** What the checks made of a take. */
export interface Verdict {
  /** True when nothing was worth mentioning. */
  readonly ok: boolean;
  /** Problems worth acting on, most important first. Empty when `ok`. */
  readonly notes: readonly string[];
}

/**
 * Judge a take against the step it was recorded for.
 *
 * Every note says what was measured and what to do, because a bare "not good
 * enough" leaves someone repeating the same take. Clipping is reported from the
 * analyser's own verdict rather than re-derived here, so the two cannot disagree.
 */
export function assess(step: CalibrationStep, detail: RecordingDetail): Verdict {
  const notes: string[] = [];
  const { source } = detail.voiceprint;
  const { minS, minVoiced, maxDriftSemitones } = step.requirements;

  if (detail.meta.clipped) {
    notes.push(
      `Clipped — ${(source.clippedFraction * 100).toFixed(1)}% of it is pinned at full scale. ` +
        `That is distortion in exactly the harmonics being measured. Move back from the mic or speak a little quieter.`,
    );
  }

  if (source.durationS < minS) {
    notes.push(
      `Only ${source.durationS.toFixed(1)}s — this step needs at least ${minS}s to measure anything stable.`,
    );
  }

  const voiced = detail.meta.voicedFraction;
  if (minVoiced !== undefined && voiced < minVoiced) {
    notes.push(
      `Only ${(voiced * 100).toFixed(0)}% of it has a pitch in it. ` +
        `Either there is a lot of silence around the sound, or it came out breathy rather than sung.`,
    );
  }

  const drift = driftSemitones(detail.voiceprint.pitch.hz);
  if (maxDriftSemitones !== undefined && drift !== null && drift > maxDriftSemitones) {
    notes.push(
      `Your pitch moved by ${drift.toFixed(1)} semitones. This step wants it held still — ` +
        `under ${maxDriftSemitones} is what the measurement needs.`,
    );
  }

  return { ok: notes.length === 0, notes };
}

/**
 * Spread of a pitch track in semitones, 5th to 95th percentile.
 *
 * Percentiles rather than the full range: a voiced frame at the very start or
 * end of phonation is often half-formed, and one of those would otherwise decide
 * whether a perfectly steady note counted as steady. Returns `null` when there
 * is too little pitch to say — an unvoiced take fails the voicing check instead,
 * which is the more useful thing to be told.
 */
export function driftSemitones(hz: readonly (number | null)[]): number | null {
  const values = hz.filter((v): v is number => v !== null && v > 0).sort((a, b) => a - b);
  if (values.length < 20) return null;

  const low = percentile(values, 0.05);
  const high = percentile(values, 0.95);
  return 12 * Math.log2(high / low);
}

/** Linear-interpolated percentile of an ascending array. */
function percentile(sorted: readonly number[], p: number): number {
  const rank = p * (sorted.length - 1);
  const lo = Math.floor(rank);
  const hi = Math.ceil(rank);
  return sorted[lo] + (sorted[hi] - sorted[lo]) * (rank - lo);
}
