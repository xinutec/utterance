import { describe, expect, it } from "vitest";

import type { Knob } from "../../models";
import {
  INITIAL_SETTINGS,
  knobValue,
  settingsQuery,
  withKnob,
  type MappingSettings,
} from "./mapping-settings";

const knob = (name: string, value: number): Knob => ({
  name,
  label: name,
  min: 0,
  max: 2,
  step: 0.05,
  default: value,
  about: "",
  mappings: [],
});

const BIND = knob("bind", 1);
const DRIFT = knob("drift", 0.25);
const KNOBS = [BIND, DRIFT];

describe("settingsQuery", () => {
  it("asks for the mapping and nothing else when nothing has been moved", () => {
    expect(settingsQuery(INITIAL_SETTINGS, KNOBS)).toBe("mapping=field");
  });

  it("carries only the knobs that were moved", () => {
    const moved = withKnob(INITIAL_SETTINGS, BIND, 0);
    expect(settingsQuery(moved, KNOBS)).toBe("mapping=field&bind=0");
  });

  it("puts the knobs in the order the backend published them", () => {
    // Two people who made the same choices must produce the same URL, or
    // comparing two renders means comparing two strings that only look
    // different.
    const a = withKnob(withKnob(INITIAL_SETTINGS, DRIFT, 1), BIND, 0);
    const b = withKnob(withKnob(INITIAL_SETTINGS, BIND, 0), DRIFT, 1);
    expect(settingsQuery(a, KNOBS)).toBe(settingsQuery(b, KNOBS));
    expect(settingsQuery(a, KNOBS)).toBe("mapping=field&bind=0&drift=1");
  });

  it("names the calibration take only when one was chosen", () => {
    const chosen: MappingSettings = { ...INITIAL_SETTINGS, calibration: "abc123" };
    expect(settingsQuery(chosen, KNOBS)).toBe("mapping=field&calibration=abc123");
  });

  it("joins several mappings the way the backend parses them", () => {
    const both: MappingSettings = { ...INITIAL_SETTINGS, mapping: ["field", "notes"] };
    expect(settingsQuery(both, KNOBS)).toBe("mapping=field%2Cnotes");
  });
});

describe("withKnob", () => {
  it("forgets a knob put back where it started", () => {
    // Otherwise exploring and then undoing leaves a URL that differs from the
    // one never touched, and two identical renders look like two different ones.
    const there = withKnob(INITIAL_SETTINGS, BIND, 0.5);
    const back = withKnob(there, BIND, BIND.default);
    expect(back.knobs).toEqual({});
    expect(settingsQuery(back, KNOBS)).toBe(settingsQuery(INITIAL_SETTINGS, KNOBS));
  });

  it("leaves the other choices alone", () => {
    const settings: MappingSettings = { ...INITIAL_SETTINGS, calibration: "abc123" };
    expect(withKnob(settings, BIND, 0).calibration).toBe("abc123");
  });
});

describe("knobValue", () => {
  it("falls back to what the backend says the knob starts at", () => {
    expect(knobValue(INITIAL_SETTINGS, DRIFT)).toBe(0.25);
    expect(knobValue(withKnob(INITIAL_SETTINGS, DRIFT, 2), DRIFT)).toBe(2);
  });
});
