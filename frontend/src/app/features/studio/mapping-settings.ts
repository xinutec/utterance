/**
 * What a listener has chosen, and how it reaches the backend.
 *
 * A separate module from the controls component because the query string is the
 * thing worth testing and worth sharing: two renders are comparable only if the
 * URL that produced each of them is, and someone who finds a setting they like
 * hands over a link rather than a description.
 */

import type { Knob, Mapping, MappingChoice } from "../../models";

/** Every choice a render depends on, beyond which take is being rendered. */
export interface MappingSettings {
  /**
   * Mappings to hear. Never empty — silence is not a choice.
   *
   * The generated union rather than `string[]`, so a name this backend does not
   * serve cannot be written here at all. It could: this was `readonly string[]`
   * while the backend's own table was a list of `&str`, and the two agreed only
   * by everyone remembering to keep them agreeing.
   */
  readonly mapping: readonly Mapping[];
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

/**
 * The settings a query string describes — the inverse of {@link settingsQuery}.
 *
 * **A URL is input from outside**, whether it was typed, edited or pasted from a
 * message, so nothing here is trusted: a knob nobody published is dropped, a
 * value that is not a number is dropped, and one outside the published range is
 * clamped to it. The alternative is a slider sitting somewhere it cannot be
 * dragged to, rendering audio the sliders on screen do not describe.
 *
 * A knob left at its default is dropped rather than recorded, so that reading a
 * link and writing it back produces the same link. Without that, opening a
 * shared comparison would immediately rewrite the address bar into a longer URL
 * saying the same thing.
 */
export function parseSettings(
  query: string,
  knobs: readonly Knob[],
  offered: readonly MappingChoice[],
  fallback: MappingSettings = INITIAL_SETTINGS,
): MappingSettings {
  const params = new URLSearchParams(query);

  // Kept only if the backend published it. The comment below has always said a
  // typo should play the default, and until the names were a type this did not
  // do that — an unknown name went through untouched and the render returned a
  // 400, so a link with one bad character played nothing at all rather than
  // playing the rest of what it asked for.
  const served = new Set<string>(offered.map((m) => m.name));
  const mapping = (params.get("mapping") ?? "")
    .split(",")
    .map((name) => name.trim())
    .filter((name): name is Mapping => served.has(name));

  const chosen: Record<string, number> = {};
  for (const knob of knobs) {
    const raw = params.get(knob.name);
    if (raw === null) continue;
    const value = Number(raw);
    if (!Number.isFinite(value)) continue;
    const clamped = Math.min(knob.max, Math.max(knob.min, value));
    if (clamped !== knob.default) chosen[knob.name] = clamped;
  }

  // An empty `calibration=` means the same as no calibration at all — let the
  // backend choose. Written out rather than leaning on truthiness, because the
  // two cases really are different values and one of them is a take id.
  const calibration = params.get("calibration");

  return {
    // Never empty: silence is not a choice, and a link with a typo in its
    // mapping name should play the default rather than nothing at all.
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
