/**
 * What each panel of a comparison shows, and how the two sides differ in it.
 *
 * **Why the difference is computed per panel rather than generically.** The
 * first version of this chart drew both scores on a shared scale and left the
 * reading to the eye. Against the comparison that matters most — the speaker's
 * tuning versus equal temperament — four of five panels were byte-identical, so
 * the second curve landed exactly on the first and hid it, and the fifth
 * differed by ten cents on an axis two octaves tall. The chart showed one line
 * and no information, which reads as a broken chart rather than as a small
 * difference.
 *
 * The fix is that every panel knows its own units and reports its own
 * difference, scaled to itself. Ten cents out of two octaves is invisible; ten
 * cents out of "the largest gap here is ten cents" is the whole panel.
 */

import type { ScoreView } from "../../models";

/** Cents in an octave, for the panels whose difference is a pitch. */
const OCTAVE_CENTS = 1200;

export interface Panel {
  readonly key: string;
  readonly label: string;
  /** Every line to draw for one side. Usually one; the pitch panel has several. */
  readonly traces: (score: ScoreView) => number[][];
  /** How far apart the two sides are at each point, in `unit`. */
  readonly difference: (a: ScoreView, b: ScoreView) => number[];
  /** What the difference is measured in, for the caption. `""` for a bare 0..1. */
  readonly unit: string;
}

/** Pointwise absolute difference, stopping at the shorter of the two. */
function apart(a: readonly number[], b: readonly number[]): number[] {
  const n = Math.min(a.length, b.length);
  return Array.from({ length: n }, (_, i) => Math.abs(a[i] - b[i]));
}

/** The interval between two frequencies, in cents. Zero where either is silent. */
function cents(from: number, to: number): number {
  if (from <= 0 || to <= 0) return 0;
  return Math.abs(OCTAVE_CENTS * Math.log2(to / from));
}

/** How far the top voice sits above the root, in octaves. */
function spread(score: ScoreView): number[] {
  const [low, high] = [score.voices.at(0), score.voices.at(-1)];
  if (!low || !high) return [];
  return low.map((hz, i) => Math.log2(Math.max(high[i], 1) / Math.max(hz, 1)));
}

export const PANELS: readonly Panel[] = [
  {
    key: "level",
    label: "level — how loud, and how many voices",
    traces: (s) => [s.level],
    difference: (a, b) => apart(a.level, b.level),
    unit: "",
  },
  {
    key: "colour",
    label: "colour — tone, dark to bright",
    traces: (s) => [s.colour],
    difference: (a, b) => apart(a.colour, b.colour),
    unit: "",
  },
  {
    // Every voice at once rather than the root alone. Under `bind` the root does
    // not move at all — the tonic is zero cents in any tuning — so a panel
    // showing only the root reports "no difference" about the one change the
    // whole knob exists to make.
    key: "pitch",
    label: "pitch — every voice, on a log axis",
    traces: (s) => s.voices.map((v) => v.map((hz) => Math.log2(Math.max(hz, 1)))),
    difference: (a, b) => {
      const voices = Math.min(a.voices.length, b.voices.length);
      if (voices === 0) return [];
      const points = Math.min(a.voices[0].length, b.voices[0].length);
      // The widest gap across the voices at each moment: one voice moving is a
      // difference even if the others hold, and averaging would dilute it.
      return Array.from({ length: points }, (_, i) =>
        Math.max(...Array.from({ length: voices }, (_, v) => cents(a.voices[v][i], b.voices[v][i]))),
      );
    },
    unit: "cents",
  },
  {
    key: "spread",
    label: "spread — how far the chord reaches above its root",
    traces: (s) => [spread(s)],
    difference: (a, b) => apart(spread(a), spread(b)).map((v) => v * OCTAVE_CENTS),
    unit: "cents",
  },
  {
    key: "breath",
    label: "breath — how much of the tone is air",
    traces: (s) => [s.breath],
    difference: (a, b) => apart(a.breath, b.breath),
    unit: "",
  },
];

/**
 * A panel's difference summarised for a caption.
 *
 * Saying "identical" out loud matters more than it looks: two curves drawn on
 * top of each other are indistinguishable from one curve, and someone reading
 * that as a rendering fault will not trust the panels that *do* differ.
 */
export function summarise(values: readonly number[], unit: string): string {
  if (values.length === 0) return "no data";
  const peak = Math.max(...values);
  if (peak === 0) return "identical";
  const figure = unit === "cents" ? peak.toFixed(0) : peak.toFixed(3);
  return `up to ${figure}${unit ? ` ${unit}` : ""}`;
}

/** The moment two scores differ most, in seconds, across every panel. */
export function mostDifferentAt(a: ScoreView, b: ScoreView): number | null {
  let best = { index: 0, share: 0 };
  for (const panel of PANELS) {
    const values = panel.difference(a, b);
    const peak = Math.max(...values, 0);
    if (peak <= 0) continue;
    values.forEach((v, i) => {
      // Compared as a share of the panel's own peak, so a panel measured in
      // cents cannot outvote one measured in 0..1 purely by its units.
      const share = v / peak;
      if (share > best.share) best = { index: i, share };
    });
  }
  return best.share > 0 ? best.index * a.stepS : null;
}
