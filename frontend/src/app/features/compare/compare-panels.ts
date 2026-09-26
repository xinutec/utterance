/**
 * What each panel of a comparison shows, and how the two sides differ in it.
 *
 * Per panel, in its own units and scaled to itself: on a shared scale, tuning
 * against equal temperament is identical in most panels and a few cents on an
 * octave-tall axis in the rest, which reads as a broken chart.
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
  const out: number[] = [];
  for (const [i, av] of a.entries()) {
    const bv = b[i];
    // Running off the end of `b` is the stopping condition.
    if (bv === undefined) break;
    out.push(Math.abs(av - bv));
  }
  return out;
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
  const out: number[] = [];
  for (const [i, hz] of low.entries()) {
    const top = high[i];
    // Same frame count in practice; stopping beats `Math.max(undefined, 1)`.
    if (top === undefined) break;
    out.push(Math.log2(Math.max(top, 1) / Math.max(hz, 1)));
  }
  return out;
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
    // Every voice, not the root: under `bind` the tonic never moves.
    key: "pitch",
    label: "pitch — every voice, on a log axis",
    traces: (s) => s.voices.map((v) => v.map((hz) => Math.log2(Math.max(hz, 1)))),
    difference: (a, b) => {
      const voices = Math.min(a.voices.length, b.voices.length);
      const [firstA, firstB] = [a.voices[0], b.voices[0]];
      if (voices === 0 || !firstA || !firstB) return [];
      const points = Math.min(firstA.length, firstB.length);
      // The widest gap across the voices: one voice moving is a difference.
      return Array.from({ length: points }, (_, i) =>
        Math.max(
          ...Array.from({ length: voices }, (_, v) => {
            const [av, bv] = [a.voices[v]?.[i], b.voices[v]?.[i]];
            // Unreachable: `v` and `i` are in bounds for both.
            return av === undefined || bv === undefined ? 0 : cents(av, bv);
          }),
        ),
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
