/**
 * Concrete colours for canvas drawing, resolved from the Material theme.
 *
 * Canvas needs a colour string it can parse. Material's system tokens are not
 * that: `--mat-sys-on-surface` computes to `light-dark(#1a1b1f, #e3e2e6)`, a CSS
 * function that only the style engine understands. Assigning it to
 * `ctx.fillStyle` fails *silently* — the property keeps whatever it held before,
 * which for a fresh context is black. That produced black text on a dark
 * background, and no error anywhere to say so.
 *
 * The way out is to make the style engine do the resolving. A real property like
 * `color` computes to a used value — an actual `rgb(...)` with `light-dark()`
 * already collapsed to the branch in force — so setting the token on a throwaway
 * element and reading `color` back gives something canvas can use.
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
 * Resolve the palette in `host`'s context.
 *
 * `host` must be attached to the document: custom properties inherit down from
 * `:root`, and an orphaned element sees none of them.
 */
export function resolveThemeColours(host: HTMLElement): ThemeColours {
  const probe = document.createElement("span");
  // Out of flow and invisible, but still rendered — `display: none` would leave
  // the computed colour unresolved in some engines.
  probe.style.position = "absolute";
  probe.style.opacity = "0";
  probe.style.pointerEvents = "none";
  host.appendChild(probe);

  try {
    const resolve = (token: string): string => {
      probe.style.color = "";
      probe.style.color = `var(${token})`;
      const resolved = getComputedStyle(probe).color;
      // An unknown token makes the declaration invalid at computed-value time,
      // so `color` falls back to the inherited one — still a legible foreground,
      // never an unparseable string.
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
 * Call `onChange` whenever the light/dark preference flips.
 *
 * A canvas holds pixels, not a stylesheet: nothing repaints it when the theme
 * changes, so without this a page left open through a switch keeps drawing in
 * the colours of the scheme it was opened in.
 *
 * Returns a teardown function.
 */
export function onColourSchemeChange(onChange: () => void): () => void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  query.addEventListener("change", onChange);
  return () => {
    query.removeEventListener("change", onChange);
  };
}
