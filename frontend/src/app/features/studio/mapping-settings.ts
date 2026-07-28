/**
 * What a listener has chosen, and how it reaches the backend.
 *
 * A separate module from the controls component because the query string is the
 * thing worth testing and worth sharing: two renders are comparable only if the
 * URL that produced each of them is, and someone who finds a setting they like
 * hands over a link rather than a description.
 */

import type { Knob } from "../../models";

/** Every choice a render depends on, beyond which take is being rendered. */
export interface MappingSettings {
  /** Mappings to hear, by name. Never empty — silence is not a choice. */
  readonly mapping: readonly string[];
  /** Take the scale comes from, or `null` to let the backend choose. */
  readonly calibration: string | null;
  /**
   * Knob values by name.
   *
   * Only knobs moved away from their default appear. That keeps the URL short
   * and, more usefully, keeps it honest: a link with nothing but `bind=0` in it
   * says exactly what was changed, where a link carrying all seven values makes
   * the interesting one impossible to spot.
   */
  readonly knobs: Readonly<Record<string, number>>;
}

/** Where a listener starts: the default mapping, the backend's calibration. */
export const INITIAL_SETTINGS: MappingSettings = {
  mapping: ["field"],
  calibration: null,
  knobs: {},
};

/**
 * The query string these settings imply, without the leading `?`.
 *
 * Built in a fixed order — mapping, calibration, then knobs as the backend
 * published them — so the same choices always produce the same URL and two of
 * them can be compared by eye.
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

/** The value a knob currently has: what was chosen, or what it starts at. */
export function knobValue(settings: MappingSettings, knob: Knob): number {
  return settings.knobs[knob.name] ?? knob.default;
}

/**
 * The settings with one knob moved.
 *
 * Setting a knob back to its default removes it rather than recording it, so
 * the URL of a knob returned to where it started is the URL of one never
 * touched — otherwise exploring and then undoing would leave a trail that makes
 * two identical renders look different.
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
