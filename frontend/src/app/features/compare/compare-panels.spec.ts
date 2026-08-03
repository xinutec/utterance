import { describe, expect, it } from "vitest";

import type { ScoreView } from "../../models";
import { PANELS, mostDifferentAt, summarise } from "./compare-panels";

const panel = (key: string) => {
  const found = PANELS.find((p) => p.key === key);
  if (!found) throw new Error(`no panel ${key}`);
  return found;
};

/** A score whose voices are a stack of fifths over a moving root. */
function score(overrides: Partial<ScoreView> = {}): ScoreView {
  const points = 100;
  const root = Array.from({ length: points }, (_, i) => 120 + i);
  return {
    durationS: 10,
    stepS: 0.1,
    colour: Array.from({ length: points }, () => 0.5),
    breath: Array.from({ length: points }, () => 0.1),
    level: Array.from({ length: points }, () => 0.8),
    voices: [root, root.map((hz) => hz * 1.5), root.map((hz) => hz * 2)],
    gains: [[], [], []],
    degrees: [0, 702, 1200],
    consonants: [],
    events: [],
    ...overrides,
  };
}

/**
 * One voice of a fixture, or a failure naming the fixture.
 *
 * `ScoreView.voices` is a bare `number[][]` on the wire — the backend does not
 * promise a voice count — so a spec reaching for voice 1 has to say what it
 * expects. Throwing beats indexing blind: the old form would have quietly
 * mapped over `undefined` had the fixture ever lost a voice.
 */
function voice(s: ScoreView, i: number): number[] {
  const found = s.voices[i];
  if (!found) throw new Error(`fixture has no voice ${i}`);
  return found;
}

/** The same score with every voice above the root raised by `cents`. */
function retuned(cents: number): ScoreView {
  const base = score();
  const ratio = 2 ** (cents / 1200);
  return {
    ...base,
    voices: [voice(base, 0), voice(base, 1).map((hz) => hz * ratio), voice(base, 2)],
  };
}

describe("the pitch panel", () => {
  it("sees a retuning the root panel alone would miss", () => {
    // The failure this was written for. Under `bind` the tonic does not move —
    // it is zero cents in every tuning — so a panel showing only the root
    // reports no difference about the one thing the knob changes.
    const difference = panel("pitch").difference(score(), retuned(15));
    expect(Math.max(...difference)).toBeCloseTo(15, 1);
  });

  it("reports the widest gap across the voices, not their average", () => {
    // One voice moving is a difference even when the others hold still.
    const difference = panel("pitch").difference(score(), retuned(90));
    expect(Math.max(...difference)).toBeCloseTo(90, 1);
  });

  it("is silent when nothing was retuned", () => {
    expect(Math.max(...panel("pitch").difference(score(), score()))).toBe(0);
  });
});

describe("summarise", () => {
  it("says identical rather than nothing", () => {
    // Two curves drawn on top of each other look like one curve, and someone
    // reading that as a rendering fault will not trust the panels that differ.
    expect(summarise([0, 0, 0], "cents")).toBe("identical");
  });

  it("carries the absolute size, since the trace is scaled to itself", () => {
    expect(summarise([0, 93.3, 12], "cents")).toBe("up to 93 cents");
    expect(summarise([0, 0.25], "")).toBe("up to 0.250");
  });

  it("says so when there is no data at all", () => {
    expect(summarise([], "cents")).toBe("no data");
  });
});

describe("mostDifferentAt", () => {
  it("finds a difference that exists only in pitch", () => {
    // The old version looked at level, colour and breath — all identical under
    // `bind` — and confidently offered second zero.
    const b = retuned(20);
    // ...and make the difference happen at one moment rather than throughout.
    b.voices[1] = voice(score(), 1).map((hz, i) => (i === 60 ? hz * 2 ** (20 / 1200) : hz));
    expect(mostDifferentAt(score(), b)).toBeCloseTo(6, 5);
  });

  it("answers null when the two are the same everywhere", () => {
    expect(mostDifferentAt(score(), score())).toBeNull();
  });

  it("weighs each panel against its own peak rather than its units", () => {
    // Cents run to hundreds and colour to one. Compared raw, a panel measured
    // in cents would always win and the colour panel would never be consulted.
    const b = score({ colour: Array.from({ length: 100 }, (_, i) => (i === 30 ? 1 : 0.5)) });
    expect(mostDifferentAt(score(), b)).toBeCloseTo(3, 5);
  });
});
