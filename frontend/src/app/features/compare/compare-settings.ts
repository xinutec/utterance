/**
 * Two settings, and what differs between them.
 *
 * The comparison is the point of this feature, so naming the difference is its
 * central operation rather than a detail of the view: someone who cannot say
 * what changed cannot say what the change did.
 */

import type { Knob } from "../../models";
import { knobValue, type MappingSettings } from "../studio/mapping-settings";

/** One knob that differs, with the value each side gave it. */
export interface Difference {
  readonly label: string;
  readonly name: string;
  readonly a: number;
  readonly b: number;
}

/** Every knob the two sides disagree about, in the order they are published. */
export function differences(
  a: MappingSettings,
  b: MappingSettings,
  knobs: readonly Knob[],
): Difference[] {
  return knobs
    .map((knob) => ({
      label: knob.label,
      name: knob.name,
      a: knobValue(a, knob),
      b: knobValue(b, knob),
    }))
    .filter((d) => d.a !== d.b);
}

/**
 * Where two streams diverge, as a 0..1 curve over the shorter of the two.
 *
 * Scaled by the largest difference rather than by the streams' own range, so
 * the curve answers *where* they differ rather than *whether* — the second is
 * already answered by the fact that anything is drawn at all. A pair that
 * differs everywhere by the same amount is a flat line at 1, which is the
 * honest picture of a change with no particular moment to it.
 */
export function divergence(a: readonly number[], b: readonly number[]): number[] {
  const n = Math.min(a.length, b.length);
  const raw = Array.from({ length: n }, (_, i) => Math.abs(a[i] - b[i]));
  const peak = Math.max(...raw, 0);
  return peak > 0 ? raw.map((v) => v / peak) : raw;
}

/**
 * The moment the two renders differ most, in seconds.
 *
 * What someone comparing two things actually wants offered to them: not a
 * verdict, but the place to listen.
 */
export function loudestDifference(divergences: readonly number[], stepS: number): number {
  let best = 0;
  for (let i = 1; i < divergences.length; i++) {
    if (divergences[i] > divergences[best]) best = i;
  }
  return best * stepS;
}
