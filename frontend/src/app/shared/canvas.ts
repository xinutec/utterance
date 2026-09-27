import { DestroyRef, type ElementRef, type Signal, afterNextRender, effect, inject, signal } from "@angular/core";

import { type ThemeColours, onColourSchemeChange, resolveThemeColours } from "./theme-colours";

/** A cleared canvas to draw one frame on, in CSS pixels. */
export interface Surface {
  readonly ctx: CanvasRenderingContext2D;
  readonly width: number;
  readonly height: number;
  readonly theme: ThemeColours;
}

/**
 * Keep a canvas painted: once it is laid out, whenever a signal `paint` reads
 * changes, on a resize and on a light/dark flip. Tracking what `paint` reads,
 * rather than a list of inputs to re-read, is what stops a chart missing one.
 * Call from a constructor.
 */
export function paintCanvas(
  canvas: Signal<ElementRef<HTMLCanvasElement>>,
  paint: (surface: Surface) => void,
): void {
  const laidOut = signal(false);
  // Bumped by what no signal sees: the element's size and the colour scheme.
  const outside = signal(0);
  const bump = (): void => {
    outside.update((n) => n + 1);
  };

  const destroyRef = inject(DestroyRef);
  afterNextRender(() => {
    const observer = new ResizeObserver(bump);
    observer.observe(canvas().nativeElement);
    const stopWatchingScheme = onColourSchemeChange(bump);
    destroyRef.onDestroy(() => {
      observer.disconnect();
      stopWatchingScheme();
    });
    laidOut.set(true);
  });

  effect(() => {
    outside();
    if (!laidOut()) return;
    const surface = prepare(canvas().nativeElement);
    if (surface) paint(surface);
  });
}

/**
 * Size the backing store to the device pixel ratio, or lines blur, and clear
 * it. `null` while the canvas has no size.
 */
function prepare(canvas: HTMLCanvasElement): Surface | null {
  const ctx = canvas.getContext("2d");
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  if (!ctx || width === 0 || height === 0) return null;

  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(width * dpr);
  canvas.height = Math.round(height * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  return { ctx, width, height, theme: resolveThemeColours(canvas.parentElement ?? canvas) };
}
