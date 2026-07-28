import { describe, expect, it } from "vitest";

import type { Knob } from "../../models";
import { INITIAL_SETTINGS, withKnob } from "../studio/mapping-settings";
import { differences } from "./compare-settings";

const knob = (name: string, label: string, def: number): Knob => ({
  name,
  label,
  min: 0,
  max: 2,
  step: 0.05,
  default: def,
  about: "",
  mappings: [],
});

const BIND = knob("bind", "Bind to the voice", 1);
const DRIFT = knob("drift", "Follow the pitch", 0.25);
const KNOBS = [BIND, DRIFT];

describe("differences", () => {
  it("finds nothing between two identical sides", () => {
    expect(differences(INITIAL_SETTINGS, INITIAL_SETTINGS, KNOBS)).toEqual([]);
  });

  it("names the knob and both values", () => {
    const b = withKnob(INITIAL_SETTINGS, BIND, 0);
    expect(differences(INITIAL_SETTINGS, b, KNOBS)).toEqual([
      { label: "Bind to the voice", name: "bind", a: "1", b: "0" },
    ]);
  });

  it("compares against the default when only one side set a knob", () => {
    // The trap: one side's `knobs` is empty, so a naive comparison of the two
    // objects would report no difference while the renders differ.
    const b = withKnob(INITIAL_SETTINGS, DRIFT, 1.5);
    const found = differences(INITIAL_SETTINGS, b, KNOBS);
    expect(found).toHaveLength(1);
    expect(found[0]).toMatchObject({ name: "drift", a: "0.25", b: "1.5" });
  });

  it("names the mapping when the two sides hear different ones", () => {
    // The largest difference the page can express, and the one it used to
    // report as "nothing differs".
    const b = { ...INITIAL_SETTINGS, mapping: ["tonnetz"] };
    expect(differences(INITIAL_SETTINGS, b, KNOBS)).toEqual([
      { label: "Mapping", name: "mapping", a: "field", b: "tonnetz" },
    ]);
  });
});
