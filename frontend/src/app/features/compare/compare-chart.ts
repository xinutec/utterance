import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnDestroy,
  effect,
  input,
  output,
  viewChild,
} from "@angular/core";

import type { ScoreView } from "../../models";
import { onColourSchemeChange, resolveThemeColours } from "../studio/theme-colours";
import { PANELS, summarise, type Panel } from "./compare-panels";

/** Height reserved above each panel for its caption. */
const LABEL_HEIGHT = 15;
/** Gap left under each panel so neighbouring series never touch. */
const PANEL_GAP = 8;
/** Share of a panel given to the difference trace under it. */
const DIFFERENCE_SHARE = 0.3;
/** Opacity of the side that is not currently audible. */
const SILENT_ALPHA = 0.4;

/**
 * Two scores on one time axis, with the difference drawn beneath each stream.
 *
 * **The difference trace is the instrument.** Compared renders are nearly the
 * same, so on the data's full range the curves overlap into one line. Each
 * panel's difference is scaled to its own largest gap and captioned with it —
 * "up to 93 cents" fills the strip — and identical panels say **identical**.
 * Canvas, for thousands of points per panel.
 */
@Component({
  selector: "app-compare-chart",
  templateUrl: "./compare-chart.html",
  styleUrl: "./compare-chart.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class CompareChart implements AfterViewInit, OnDestroy {
  readonly a = input.required<ScoreView>();
  readonly b = input.required<ScoreView>();
  /** Where the players are, in seconds, so the chart can show a playhead. */
  readonly playhead = input<number>(0);
  /** Which side is audible; it is drawn solid and the other faded. */
  readonly audible = input<"a" | "b">("a");

  /** A click on the chart, in seconds. Both players seek here. */
  readonly seek = output<number>();

  private readonly canvasRef = viewChild.required<ElementRef<HTMLCanvasElement>>("canvas");
  private observer?: ResizeObserver;
  private stopWatchingScheme?: () => void;

  constructor() {
    effect(() => {
      // Read every input, so the effect re-runs when any moves.
      const inputs = [this.a(), this.b(), this.playhead(), this.audible()] as const;
      if (this.observer) this.draw(...inputs);
    });
  }

  ngAfterViewInit(): void {
    this.observer = new ResizeObserver(() => this.redraw());
    this.observer.observe(this.canvasRef().nativeElement);
    this.stopWatchingScheme = onColourSchemeChange(() => this.redraw());
    this.redraw();
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
    this.stopWatchingScheme?.();
  }

  onClick(event: MouseEvent): void {
    const canvas = this.canvasRef().nativeElement;
    const box = canvas.getBoundingClientRect();
    this.seek.emit(((event.clientX - box.left) / box.width) * this.duration());
  }

  private duration(): number {
    return Math.max(this.a().durationS, this.b().durationS, 0.001);
  }

  private redraw(): void {
    this.draw(this.a(), this.b(), this.playhead(), this.audible());
  }

  private draw(a: ScoreView, b: ScoreView, playhead: number, audible: "a" | "b"): void {
    const canvas = this.canvasRef().nativeElement;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (width === 0 || height === 0) return;

    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);

    const theme = resolveThemeColours(canvas.parentElement ?? canvas);
    const panelHeight = height / PANELS.length;

    PANELS.forEach((panel, index) => {
      const top = index * panelHeight;
      const difference = panel.difference(a, b);

      this.caption(ctx, panel, difference, top, width, theme);

      const plotTop = top + LABEL_HEIGHT;
      const usable = panelHeight - LABEL_HEIGHT - PANEL_GAP;
      const diffHeight = usable * DIFFERENCE_SHARE;
      const plotHeight = usable - diffHeight;

      const tracesA = panel.traces(a);
      const tracesB = panel.traces(b);
      const scale = bounds([...tracesA, ...tracesB], panel.key);

      // Each side keeps its colour; only the draw order changes.
      const sideA = [tracesA, theme.accent] as const;
      const sideB = [tracesB, theme.warm] as const;
      // The silent side first and faded, so the audible one is on top.
      const layers = audible === "a" ? [sideB, sideA] : [sideA, sideB];
      layers.forEach(([traces, colour], i) => {
        ctx.globalAlpha = i === 0 ? SILENT_ALPHA : 1;
        for (const trace of traces) {
          this.plot(ctx, trace, scale, width, plotTop, plotHeight, colour);
        }
      });
      ctx.globalAlpha = 1;

      this.difference(ctx, difference, width, plotTop + plotHeight, diffHeight, theme.ink);

      ctx.strokeStyle = theme.muted;
      ctx.globalAlpha = 0.25;
      ctx.beginPath();
      ctx.moveTo(0, top + panelHeight - PANEL_GAP);
      ctx.lineTo(width, top + panelHeight - PANEL_GAP);
      ctx.stroke();
      ctx.globalAlpha = 1;
    });

    // The playhead last, over everything.
    const x = (playhead / this.duration()) * width;
    ctx.strokeStyle = theme.ink;
    ctx.globalAlpha = 0.55;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
    ctx.globalAlpha = 1;
  }

  /** The panel's name, and how far apart the two sides are in it. */
  private caption(
    ctx: CanvasRenderingContext2D,
    panel: Panel,
    difference: readonly number[],
    top: number,
    width: number,
    theme: { muted: string; ink: string },
  ): void {
    ctx.font = "11px system-ui, sans-serif";
    ctx.textAlign = "left";
    ctx.fillStyle = theme.muted;
    ctx.fillText(panel.label, 0, top + 11);

    const verdict = summarise(difference, panel.unit);
    ctx.textAlign = "right";
    // "identical" in full-strength ink: it must not be missed.
    ctx.fillStyle = verdict === "identical" ? theme.ink : theme.muted;
    ctx.fillText(verdict, width, top + 11);
    ctx.textAlign = "left";
  }

  private plot(
    ctx: CanvasRenderingContext2D,
    values: readonly number[],
    [lo, hi]: readonly [number, number],
    width: number,
    top: number,
    height: number,
    colour: string,
  ): void {
    if (values.length === 0) return;
    const span = hi - lo || 1;
    ctx.strokeStyle = colour;
    ctx.lineWidth = 1.75;
    ctx.lineJoin = "round";
    ctx.beginPath();
    values.forEach((v, i) => {
      const x = (i / Math.max(1, values.length - 1)) * width;
      const y = top + height - ((v - lo) / span) * height;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
  }

  /**
   * How far apart the two sides are, filled and scaled to its own largest gap,
   * with the caption giving the real size.
   */
  private difference(
    ctx: CanvasRenderingContext2D,
    values: readonly number[],
    width: number,
    top: number,
    height: number,
    colour: string,
  ): void {
    if (values.length === 0 || height <= 0) return;
    const peak = Math.max(...values);
    if (peak <= 0) return;

    const step = width / values.length;
    ctx.fillStyle = colour;
    values.forEach((v, i) => {
      const share = v / peak;
      if (share <= 0) return;
      ctx.globalAlpha = 0.2 + 0.5 * share;
      ctx.fillRect(i * step, top + height * (1 - share), Math.max(step, 1), height * share);
    });
    ctx.globalAlpha = 1;
  }
}

/**
 * The range a panel is drawn against, shared by both sides — scaled apart, two
 * series an octave apart would overlap.
 */
function bounds(traces: readonly (readonly number[])[], key: string): readonly [number, number] {
  // Colour and breath pinned to 0..1, so a small difference stays small.
  if (key === "colour" || key === "breath") return [0, 1];

  let lo = Infinity;
  let hi = -Infinity;
  for (const trace of traces) {
    for (const v of trace) {
      if (v < lo) lo = v;
      if (v > hi) hi = v;
    }
  }
  if (!Number.isFinite(lo)) return [0, 1];
  // A flat pair still needs some height.
  return hi > lo ? [lo, hi] : [lo - 0.5, lo + 0.5];
}
