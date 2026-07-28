/**
 * Two settings, and which knobs they disagree about.
 *
 * Naming the difference is the feature's central operation rather than a detail
 * of the view: someone who cannot say what changed cannot say what the change
 * did. Comparing the resulting *streams* is `compare-panels.ts` next door.
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
 * Everything the two sides disagree about, in the order it is published.
 *
 * **The mapping counts as a setting**, and it is now the largest one there is:
 * the field and the lattice are two different pieces of music from one voice,
 * where a knob is a shade of one. Left out, the page would answer "nothing
 * differs" to the most interesting comparison it can make — and since that
 * sentence is the only thing telling a listener what they are listening for,
 * being silent about it is worse than being wrong about a knob.
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
