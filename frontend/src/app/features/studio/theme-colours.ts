/**
 * Concrete colours for canvas drawing, resolved from the Material theme.
 *
 * Material's tokens compute to `light-dark(…)`, which canvas cannot parse and
 * silently ignores, keeping black. So the token is set as `color` on a
 * throwaway element and read back resolved, as an `rgb(...)`.
 */

/** The palette every canvas in this app draws with. */
const TOKENS = {
  /** Primary text and foreground marks. */
  ink: "--mat-sys-on-surface",
  /** Axes, gridlines, captions. */
  muted: "--mat-sys-outline",
  /** The main data series. */
  accent: "--mat-sys-primary",
  /** A second series that must stay distinguishable from the first. */
  warm: "--mat-sys-tertiary",
} as const;

export type ThemeColours = Record<keyof typeof TOKENS, string>;

/**
 * Resolve the palette in `host`'s context; `host` must be attached, or it
 * inherits no custom properties.
 */
export function resolveThemeColours(host: HTMLElement): ThemeColours {
  const probe = document.createElement("span");
  // Hidden but rendered: `display: none` can leave the colour unresolved.
  probe.style.position = "absolute";
  probe.style.opacity = "0";
  probe.style.pointerEvents = "none";
  host.appendChild(probe);

  try {
    const resolve = (token: string): string => {
      probe.style.color = "";
      probe.style.color = `var(${token})`;
      const resolved = getComputedStyle(probe).color;
      // An unknown token falls back to the inherited colour: legible, parseable.
      return resolved || "#888888";
    };

    return {
      ink: resolve(TOKENS.ink),
      muted: resolve(TOKENS.muted),
      accent: resolve(TOKENS.accent),
      warm: resolve(TOKENS.warm),
    };
  } finally {
    probe.remove();
  }
}

/**
 * Call `onChange` whenever the light/dark preference flips — nothing repaints a
 * canvas otherwise. Returns a teardown function.
 */
export function onColourSchemeChange(onChange: () => void): () => void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  query.addEventListener("change", onChange);
  return () => {
    query.removeEventListener("change", onChange);
  };
}
