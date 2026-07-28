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
import { divergence } from "./compare-settings";

/** Height reserved above each panel for its caption. */
const LABEL_HEIGHT = 14;
/** Gap left under each panel so neighbouring series never touch. */
const PANEL_GAP = 6;

/** The streams worth putting side by side, and what each one answers. */
const PANELS = [
  { key: "level", label: "level — how loud, and how many voices" },
  { key: "colour", label: "colour — tone, dark to bright" },
  { key: "root", label: "root — the pitch everything is built on (log)" },
  { key: "spread", label: "spread — how far the chord reaches above it" },
  { key: "breath", label: "breath — how much of the tone is air" },
] as const;

type PanelKey = (typeof PANELS)[number]["key"];

/**
 * Two scores drawn on one time axis, with where they differ marked.
 *
 * **Why this exists.** Two renders a listener cannot tell apart are not
 * necessarily two renders that are the same, and the difference between those
 * two situations decides whether a knob is worth keeping. Drawing both makes the
 * question answerable: either the lines separate somewhere, and that somewhere
 * is where to listen, or they do not, and the knob does nothing audible however
 * much the bytes differ.
 *
 * The divergence strip under each panel is the actual instrument here. It says
 * *when* rather than *whether*, and clicking it moves both players there —
 * which turns "hard to tell" into "listen at 12.4 seconds".
 *
 * Canvas rather than SVG, like the voiceprint chart next door: a thousand points
 * per stream across five panels is more DOM than a page can carry.
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

  /** A click on the chart, in seconds. Both players seek here. */
  readonly seek = output<number>();

  private readonly canvasRef = viewChild.required<ElementRef<HTMLCanvasElement>>("canvas");
  private observer?: ResizeObserver;
  private stopWatchingScheme?: () => void;

  constructor() {
    effect(() => {
      // Read every input so the effect re-runs when any of them moves.
      const [a, b, at] = [this.a(), this.b(), this.playhead()];
      if (this.observer) this.draw(a, b, at);
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
    const bounds = canvas.getBoundingClientRect();
    const fraction = (event.clientX - bounds.left) / bounds.width;
    this.seek.emit(fraction * this.duration());
  }

  private duration(): number {
    return Math.max(this.a().durationS, this.b().durationS, 0.001);
  }

  private redraw(): void {
    this.draw(this.a(), this.b(), this.playhead());
  }

  private draw(a: ScoreView, b: ScoreView, playhead: number): void {
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

    const { ink, muted, accent, warm } = resolveThemeColours(canvas.parentElement ?? canvas);

    const panelHeight = height / PANELS.length;
    let top = 0;
    for (const panel of PANELS) {
      const seriesA = series(a, panel.key);
      const seriesB = series(b, panel.key);

      ctx.fillStyle = muted;
      ctx.font = "11px system-ui, sans-serif";
      ctx.fillText(panel.label, 0, top + 11);

      const plotTop = top + LABEL_HEIGHT;
      // The bottom eighth of each panel is the divergence strip.
      const strip = (panelHeight - LABEL_HEIGHT - PANEL_GAP) * 0.18;
      const plotHeight = panelHeight - LABEL_HEIGHT - PANEL_GAP - strip;

      const scale = bounds(seriesA, seriesB, panel.key);
      this.plot(ctx, seriesA, scale, width, plotTop, plotHeight, accent, 1.75);
      this.plot(ctx, seriesB, scale, width, plotTop, plotHeight, warm, 1.75);
      this.strip(ctx, divergence(seriesA, seriesB), width, plotTop + plotHeight, strip, ink);

      ctx.strokeStyle = muted;
      ctx.globalAlpha = 0.25;
      ctx.beginPath();
      ctx.moveTo(0, top + panelHeight - PANEL_GAP);
      ctx.lineTo(width, top + panelHeight - PANEL_GAP);
      ctx.stroke();
      ctx.globalAlpha = 1;

      top += panelHeight;
    }

    // The playhead last, over everything, so it is never hidden by a series.
    const x = (playhead / this.duration()) * width;
    ctx.strokeStyle = ink;
    ctx.globalAlpha = 0.55;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
    ctx.globalAlpha = 1;
  }

  private plot(
    ctx: CanvasRenderingContext2D,
    values: readonly number[],
    [lo, hi]: readonly [number, number],
    width: number,
    top: number,
    height: number,
    colour: string,
    lineWidth: number,
  ): void {
    if (values.length === 0) return;
    const span = hi - lo || 1;
    ctx.strokeStyle = colour;
    ctx.lineWidth = lineWidth;
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
   * Where the two differ, as a filled strip.
   *
   * Filled rather than drawn as a line because it is not a measurement anyone
   * reads a value off — it is a heat map with one dimension, and the only
   * question asked of it is *where is it tallest*.
   */
  private strip(
    ctx: CanvasRenderingContext2D,
    values: readonly number[],
    width: number,
    top: number,
    height: number,
    colour: string,
  ): void {
    if (values.length === 0 || height <= 0) return;
    const step = width / values.length;
    ctx.fillStyle = colour;
    values.forEach((v, i) => {
      if (v <= 0) return;
      ctx.globalAlpha = 0.15 + 0.55 * v;
      ctx.fillRect(i * step, top + height * (1 - v), Math.max(step, 1), height * v);
    });
    ctx.globalAlpha = 1;
  }
}

/** One panel's series, derived from a score. */
function series(score: ScoreView, key: PanelKey): number[] {
  switch (key) {
    case "level":
      return score.level;
    case "colour":
      return score.colour;
    case "breath":
      return score.breath;
    // The lowest voice is the root everything else is stacked on. Log, because
    // this is a pitch and pitch is heard in ratios.
    case "root":
      return score.voices.length > 0 ? score.voices[0].map((hz) => Math.log2(Math.max(hz, 1))) : [];
    // How far the top voice sits above the root, in octaves. This is the number
    // `voicing` and `spacing` move, and it is invisible in either voice alone.
    case "spread": {
      const [low, high] = [score.voices.at(0), score.voices.at(-1)];
      if (!low || !high) return [];
      return low.map((hz, i) => Math.log2(Math.max(high[i], 1) / Math.max(hz, 1)));
    }
  }
}

/**
 * The range a panel is drawn against, shared by both sides.
 *
 * Shared is the whole point: scaled independently, two series that differ by an
 * octave would be drawn on top of each other and the chart would report no
 * difference at all.
 */
function bounds(a: readonly number[], b: readonly number[], key: PanelKey): readonly [number, number] {
  // Colour and breath are defined on 0..1, and pinning them there keeps a small
  // real difference small rather than magnifying it to fill the panel.
  if (key === "colour" || key === "breath") return [0, 1];

  const all = [...a, ...b];
  if (all.length === 0) return [0, 1];
  const lo = Math.min(...all);
  const hi = Math.max(...all);
  // A flat pair still needs a panel's worth of height to sit in the middle of.
  return hi > lo ? [lo, hi] : [lo - 0.5, lo + 0.5];
}
