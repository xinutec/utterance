/**
 * What a listener has chosen, and the query string that carries it — the thing
 * worth testing and sharing, since a setting is passed on as a link.
 */

import type { Knob, KnobName, Mapping, MappingChoice } from "../../models";

/** Every choice a render depends on, beyond which take is being rendered. */
export interface MappingSettings {
  /** Mappings to hear; never empty. Typed by the generated union. */
  readonly mapping: readonly Mapping[];
  /** Take the scale comes from, or `null` to let the backend choose. */
  readonly calibration: string | null;
  /**
   * Knob values by name, only those moved from their default — so a link says
   * exactly what changed. Keys are the generated `KnobName`, so an unpublished
   * knob cannot be written; absent means at its default.
   */
  readonly knobs: Readonly<Partial<Record<KnobName, number>>>;
}

/** Where a listener starts: the default mapping, the backend's calibration. */
export const INITIAL_SETTINGS: MappingSettings = {
  mapping: ["field"],
  calibration: null,
  knobs: {},
};

/**
 * The query string these settings imply, without the leading `?`, in a fixed
 * order so the same choices always give the same URL.
 */
export function settingsQuery(settings: MappingSettings, knobs: readonly Knob[]): string {
  const query = new URLSearchParams();
  if (settings.mapping.length > 0) query.set("mapping", settings.mapping.join(","));
  if (settings.calibration) query.set("calibration", settings.calibration);
  for (const knob of knobs) {
    const value = settings.knobs[knob.name];
    if (value !== undefined) query.set(knob.name, String(value));
  }
  return query.toString();
}

/**
 * The settings a query string describes — the inverse of {@link settingsQuery}.
 *
 * **A URL is input from outside**: an unpublished knob or a non-number is
 * dropped, an out-of-range value clamped. A knob at its default is dropped too,
 * so reading a link and writing it back gives the same link.
 */
export function parseSettings(
  query: string,
  knobs: readonly Knob[],
  offered: readonly MappingChoice[],
  fallback: MappingSettings = INITIAL_SETTINGS,
): MappingSettings {
  const params = new URLSearchParams(query);

  // Only published names: an unknown one would make the render 400 and lose
  // the rest of the link.
  const served = new Set<string>(offered.map((m) => m.name));
  const mapping = (params.get("mapping") ?? "")
    .split(",")
    .map((name) => name.trim())
    .filter((name): name is Mapping => served.has(name));

  const chosen: Partial<Record<KnobName, number>> = {};
  for (const knob of knobs) {
    const raw = params.get(knob.name);
    if (raw === null) continue;
    const value = Number(raw);
    if (!Number.isFinite(value)) continue;
    const clamped = Math.min(knob.max, Math.max(knob.min, value));
    if (clamped !== knob.default) chosen[knob.name] = clamped;
  }

  // An empty `calibration=` means none: let the backend choose.
  const calibration = params.get("calibration");

  return {
    // Never empty: a typo in the mapping name plays the default.
    mapping: mapping.length > 0 ? mapping : fallback.mapping,
    calibration: calibration === null || calibration === "" ? null : calibration,
    knobs: chosen,
  };
}

/** The value a knob currently has: what was chosen, or what it starts at. */
export function knobValue(settings: MappingSettings, knob: Knob): number {
  return settings.knobs[knob.name] ?? knob.default;
}

/**
 * The settings with one knob moved. Moving it back to its default removes it,
 * so undoing leaves the URL of a knob never touched.
 */
export function withKnob(
  settings: MappingSettings,
  knob: Knob,
  value: number,
): MappingSettings {
  const knobs = { ...settings.knobs };
  if (value === knob.default) delete knobs[knob.name];
  else knobs[knob.name] = value;
  return { ...settings, knobs };
}
