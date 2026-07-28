import { describe, expect, it } from "vitest";

import type { Knob } from "../../models";
import { INITIAL_SETTINGS, withKnob } from "../studio/mapping-settings";
import { differences, divergence, loudestDifference } from "./compare-settings";

const knob = (name: string, label: string, def: number): Knob => ({
  name,
  label,
  min: 0,
  max: 2,
  step: 0.05,
  default: def,
  about: "",
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
      { label: "Bind to the voice", name: "bind", a: 1, b: 0 },
    ]);
  });

  it("compares against the default when only one side set a knob", () => {
    // The trap: one side's `knobs` is empty, so a naive comparison of the two
    // objects would report no difference while the renders differ.
    const b = withKnob(INITIAL_SETTINGS, DRIFT, 1.5);
    const found = differences(INITIAL_SETTINGS, b, KNOBS);
    expect(found).toHaveLength(1);
    expect(found[0]).toMatchObject({ name: "drift", a: 0.25, b: 1.5 });
  });
});

describe("divergence", () => {
  it("is flat zero for two identical streams", () => {
    expect(divergence([1, 2, 3], [1, 2, 3])).toEqual([0, 0, 0]);
  });

  it("peaks where the two differ most", () => {
    const d = divergence([0, 0, 0, 0], [0, 0.5, 0, 0]);
    expect(d).toEqual([0, 1, 0, 0]);
  });

  it("reports a constant difference as constant rather than as nothing", () => {
    // Scaled by its own peak, so a pair differing everywhere by the same amount
    // is a flat line at 1. Scaling by range would make it a flat zero, which
    // reads as "no difference" — the opposite of the truth.
    expect(divergence([0, 0, 0], [1, 1, 1])).toEqual([1, 1, 1]);
  });

  it("stops at the shorter of the two", () => {
    expect(divergence([0, 0, 0], [1, 1])).toHaveLength(2);
  });
});

describe("loudestDifference", () => {
  it("converts the peak's index into seconds", () => {
    expect(loudestDifference([0, 0.2, 0.9, 0.1], 0.5)).toBe(1);
  });

  it("answers zero when nothing differs, rather than failing", () => {
    expect(loudestDifference([0, 0, 0], 0.5)).toBe(0);
  });
});
