/**
 * Two settings, and which knobs they disagree about — someone who cannot say
 * what changed cannot say what it did. Comparing the streams is
 * `compare-panels.ts`.
 */

import type { Knob } from "../../models";
import { knobValue, type MappingSettings } from "../studio/mapping-settings";

/** One setting that differs, with what each side made of it. */
export interface Difference {
  readonly label: string;
  readonly name: string;
  readonly a: string;
  readonly b: string;
}

/**
 * Everything the two sides disagree about, in published order. The mapping
 * counts, and is the largest difference there is; leaving it out would say
 * "nothing differs" about the most interesting comparison.
 */
export function differences(
  a: MappingSettings,
  b: MappingSettings,
  knobs: readonly Knob[],
): Difference[] {
  const mapping = {
    label: "Mapping",
    name: "mapping",
    a: a.mapping.join(" + "),
    b: b.mapping.join(" + "),
  };
  return [
    mapping,
    ...knobs.map((knob) => ({
      label: knob.label,
      name: knob.name,
      a: String(knobValue(a, knob)),
      b: String(knobValue(b, knob)),
    })),
  ].filter((d) => d.a !== d.b);
}
