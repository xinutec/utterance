import { describe, expect, it } from "vitest";

import type { Knob } from "../../models";
import {
  INITIAL_SETTINGS,
  knobValue,
  parseSettings,
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
  primary: true,
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

describe("parseSettings", () => {
  it("reads back exactly what settingsQuery wrote", () => {
    // The property the whole feature rests on: a comparison someone shares is
    // the comparison the other person hears. Anything asymmetric here is two
    // people listening to two different things while believing otherwise.
    const settings: MappingSettings = {
      mapping: ["tonnetz"],
      calibration: "3c42eea98f207a41",
      knobs: { bind: 0, drift: 0.5 },
    };
    expect(parseSettings(settingsQuery(settings, KNOBS), KNOBS)).toEqual(settings);
  });

  it("leaves a link alone when it is opened and written straight back", () => {
    // A knob at its default is dropped, so reading and re-writing is a fixed
    // point. Without it, opening a shared link would rewrite the address bar
    // into a longer URL saying the same thing.
    const link = "mapping=tonnetz&bind=0";
    expect(settingsQuery(parseSettings(link, KNOBS), KNOBS)).toBe(link);
  });

  it("drops a knob nobody published", () => {
    // A URL is input from outside. An unknown name means a stale link or a
    // typo, and passing it through would send the backend a parameter it
    // rejects — losing the whole comparison over one word.
    const parsed = parseSettings("mapping=field&nonsense=3", KNOBS);
    expect(parsed.knobs).toEqual({});
  });

  it("drops a value that is not a number", () => {
    expect(parseSettings("mapping=field&bind=loud", KNOBS).knobs).toEqual({});
  });

  it("clamps a value outside the published range", () => {
    // Rather than dropping it: someone who wrote bind=5 meant the top of the
    // range, and a slider that cannot show what is playing is the failure the
    // knob table exists to prevent.
    expect(parseSettings("mapping=field&bind=5", KNOBS).knobs).toEqual({ bind: 2 });
    expect(parseSettings("mapping=field&drift=-1", KNOBS).knobs).toEqual({ drift: 0 });
  });

  it("falls back to the default mapping rather than to silence", () => {
    expect(parseSettings("", KNOBS).mapping).toEqual(INITIAL_SETTINGS.mapping);
    expect(parseSettings("mapping=", KNOBS).mapping).toEqual(INITIAL_SETTINGS.mapping);
  });

  it("keeps several mappings", () => {
    expect(parseSettings("mapping=field,compose", KNOBS).mapping).toEqual(["field", "compose"]);
  });
})
