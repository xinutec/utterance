import { describe, expect, it } from "vitest";

import type { RecordingDetail } from "../../models";
import { STEPS, assess, driftSemitones, type CalibrationStep } from "./steps";

const step = (id: string): CalibrationStep => {
  const found = STEPS.find((s) => s.id === id);
  if (!found) throw new Error(`no such step: ${id}`);
  return found;
};

/** A take that passes every check, so each test can spoil exactly one thing. */
function goodTake(overrides: Partial<Fixture> = {}): RecordingDetail {
  const f: Fixture = {
    durationS: 10,
    voicedFraction: 0.9,
    clipped: false,
    clippedFraction: 0,
    hz: Array.from({ length: 900 }, () => 135),
    ...overrides,
  };

  return {
    meta: {
      id: "0123456789abcdef",
      label: "steady-ah",
      createdAtMs: 0,
      durationS: f.durationS,
      sampleRateHz: 44100,
      voicedFraction: f.voicedFraction,
      onsetCount: 1,
      peak: 0.5,
      clipped: f.clipped,
    },
    voiceprint: {
      schemaVersion: 4,
      source: {
        sampleRateHz: 44100,
        channels: 1,
        durationS: f.durationS,
        peak: 0.5,
        clippedFraction: f.clippedFraction,
      },
      frame: { analysisRateHz: 16000, hopS: 0.01, count: f.hz.length },
      pitch: { hz: f.hz, aperiodicity: f.hz.map(() => 0.1) },
      formants: { f1: [], f2: [], f3: [] },
      rmsDb: [],
      events: { flux: [], onsetFrames: [], onsetTimesS: [] },
    },
  };
}

interface Fixture {
  durationS: number;
  voicedFraction: number;
  clipped: boolean;
  clippedFraction: number;
  hz: (number | null)[];
}

describe("driftSemitones", () => {
  it("reports no movement for a perfectly held pitch", () => {
    expect(driftSemitones(Array.from({ length: 100 }, () => 135))).toBeCloseTo(0, 6);
  });

  it("measures an octave as twelve semitones", () => {
    // Ramping 100→200 Hz, so the 5th and 95th percentiles sit inside the octave
    // and the answer lands just under 12.
    const hz = Array.from({ length: 1000 }, (_, i) => 100 * 2 ** (i / 999));
    expect(driftSemitones(hz)).toBeGreaterThan(10.5);
    expect(driftSemitones(hz)).toBeLessThan(12);
  });

  it("ignores unvoiced frames rather than reading them as zero", () => {
    const withGaps = [...Array.from({ length: 50 }, () => 135), ...Array.from({ length: 50 }, () => null)];
    expect(driftSemitones(withGaps)).toBeCloseTo(0, 6);
  });

  it("says nothing when there is too little pitch to judge", () => {
    expect(driftSemitones([135, null, 136])).toBeNull();
    expect(driftSemitones([])).toBeNull();
  });

  it("is not thrown off by one stray frame", () => {
    // Percentiles rather than the full range: a half-formed frame at the edge of
    // phonation must not decide whether a steady note counted as steady.
    const hz = [...Array.from({ length: 200 }, () => 135), 400];
    expect(driftSemitones(hz)).toBeLessThan(0.5);
  });
});

describe("assess", () => {
  it("passes a take that does what the step asked", () => {
    expect(assess(step("steady-ah"), goodTake())).toEqual({ ok: true, notes: [] });
  });

  it("flags clipping with the measured number and something to do", () => {
    const v = assess(step("steady-ah"), goodTake({ clipped: true, clippedFraction: 0.023 }));
    expect(v.ok).toBe(false);
    expect(v.notes[0]).toContain("2.3%");
    expect(v.notes[0]).toMatch(/quieter|back/);
  });

  it("flags a take too short for its step", () => {
    const v = assess(step("steady-ah"), goodTake({ durationS: 2 }));
    expect(v.ok).toBe(false);
    expect(v.notes.join(" ")).toContain("2.0s");
  });

  it("flags a take with too little voicing in it", () => {
    const v = assess(step("steady-ah"), goodTake({ voicedFraction: 0.1 }));
    expect(v.ok).toBe(false);
    expect(v.notes.join(" ")).toContain("10%");
  });

  it("flags a wandering pitch where the step wanted it held", () => {
    const hz = Array.from({ length: 900 }, (_, i) => 120 + (i / 899) * 40);
    const v = assess(step("steady-ah"), goodTake({ hz }));
    expect(v.ok).toBe(false);
    expect(v.notes.join(" ")).toMatch(/semitones/);
  });

  it("does not judge steadiness where the step is about the note, not the hold", () => {
    // The pitch-range steps are sustained too, but a wobble on someone's lowest
    // comfortable note says nothing about whether they reached it.
    const hz = Array.from({ length: 900 }, (_, i) => 90 + (i / 899) * 30);
    const v = assess(step("pitch-low"), goodTake({ hz, durationS: 3 }));
    expect(v.ok).toBe(true);
  });

  it("reports every problem at once rather than one at a time", () => {
    const v = assess(
      step("steady-ah"),
      goodTake({ durationS: 1, clipped: true, clippedFraction: 0.05, voicedFraction: 0.05 }),
    );
    expect(v.notes.length).toBe(3);
  });

  it("accepts spontaneous speech, which is voiced far less than a held vowel", () => {
    const v = assess(step("speech"), goodTake({ durationS: 40, voicedFraction: 0.3 }));
    expect(v.ok).toBe(true);
  });
});

describe("STEPS", () => {
  it("has unique ids, since they are used as take labels", () => {
    expect(new Set(STEPS.map((s) => s.id)).size).toBe(STEPS.length);
  });

  it("never suggests a length its own check would reject", () => {
    for (const s of STEPS) {
      expect(s.targetS, s.id).toBeGreaterThanOrEqual(s.requirements.minS);
    }
  });

  it("opens with the take the tuning is derived from", () => {
    // Ordering is load-bearing: if only one step gets done, this is the one that
    // unblocks the partial-ratio measurement.
    expect(STEPS[0].id).toBe("steady-ah");
  });
});
