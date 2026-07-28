/**
 * Two settings, and which knobs they disagree about.
 *
 * Naming the difference is the feature's central operation rather than a detail
 * of the view: someone who cannot say what changed cannot say what the change
 * did. Comparing the resulting *streams* is `compare-panels.ts` next door.
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
