import {
  AfterViewInit,
  Component,
  ElementRef,
  OnDestroy,
  ChangeDetectionStrategy,
  effect,
  input,
  viewChild,
} from "@angular/core";

import type { Voiceprint } from "../../models";

/** Lowest and highest frequency drawn on the pitch panel — the tracker's range. */
const PITCH_MIN_HZ = 70;
const PITCH_MAX_HZ = 500;

/** Level range drawn on the energy panel, in dBFS. */
const LEVEL_FLOOR_DB = -70;
const LEVEL_CEILING_DB = 0;

/** Panel heights as fractions of the canvas, top to bottom. */
const PANELS = [
  { key: "pitch", label: "pitch — prosodic contour", weight: 0.45 },
  { key: "level", label: "level — phrasing", weight: 0.3 },
  { key: "flux", label: "spectral flux — events", weight: 0.25 },
] as const;

/**
 * Renders a voiceprint as three stacked time-aligned panels.
 *
 * Everything shares one x-axis and one frame grid, because reading these
 * together is the point: an onset that lands nowhere near an energy rise, or a
 * pitch reading in a frame with no level, is how you spot the analyser being
 * wrong. Separate charts with independent axes would hide exactly that.
 *
 * Canvas rather than SVG: half a minute of speech is a few thousand frames per
 * series, and that many DOM nodes makes the page unusable.
 */
@Component({
  selector: "app-voiceprint-chart",
  templateUrl: "./voiceprint-chart.html",
  styleUrl: "./voiceprint-chart.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class VoiceprintChart implements AfterViewInit, OnDestroy {
  readonly voiceprint = input.required<Voiceprint>();

  private readonly canvasRef = viewChild.required<ElementRef<HTMLCanvasElement>>("canvas");
  private observer?: ResizeObserver;

  constructor() {
    // Redraws whenever the input changes; the first draw waits for the view,
    // since there is no canvas to measure until then.
    effect(() => {
      const vp = this.voiceprint();
      if (this.observer) this.draw(vp);
    });
  }

  ngAfterViewInit(): void {
    this.observer = new ResizeObserver(() => {
      this.draw(this.voiceprint());
    });
    this.observer.observe(this.canvasRef().nativeElement);
    this.draw(this.voiceprint());
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
  }

  private draw(vp: Voiceprint): void {
    const canvas = this.canvasRef().nativeElement;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Match the backing store to the device pixel ratio, or every line is
    // blurred on a retina display.
    const dpr = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (width === 0 || height === 0) return;

    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);

    const style = getComputedStyle(canvas);
    const ink = style.getPropertyValue("--chart-ink").trim() || "#e0e0e0";
    const muted = style.getPropertyValue("--chart-muted").trim() || "#6a6a6a";
    const accent = style.getPropertyValue("--chart-accent").trim() || "#7cc4ff";
    const warm = style.getPropertyValue("--chart-warm").trim() || "#ffb26b";

    const count = vp.frame.count;
    if (count === 0) return;
    const x = (frame: number): number => (frame / Math.max(1, count - 1)) * width;

    let top = 0;
    for (const panel of PANELS) {
      const h = height * panel.weight;
      this.drawPanelFrame(ctx, panel.label, top, h, width, muted);

      const plotTop = top + LABEL_HEIGHT;
      const plotHeight = h - LABEL_HEIGHT - PANEL_GAP;

      if (panel.key === "pitch") this.drawPitch(ctx, vp, x, plotTop, plotHeight, accent);
      if (panel.key === "level") this.drawLevel(ctx, vp, x, plotTop, plotHeight, ink);
      if (panel.key === "flux") this.drawFlux(ctx, vp, x, plotTop, plotHeight, ink, warm);

      top += h;
    }
  }

  private drawPanelFrame(
    ctx: CanvasRenderingContext2D,
    label: string,
    top: number,
    height: number,
    width: number,
    muted: string,
  ): void {
    ctx.fillStyle = muted;
    ctx.font = "11px system-ui, sans-serif";
    ctx.fillText(label, 0, top + 11);

    ctx.strokeStyle = muted;
    ctx.globalAlpha = 0.25;
    ctx.beginPath();
    ctx.moveTo(0, top + height - PANEL_GAP);
    ctx.lineTo(width, top + height - PANEL_GAP);
    ctx.stroke();
    ctx.globalAlpha = 1;
  }

  /**
   * Pitch on a log axis, because pitch is perceived in ratios: an octave should
   * occupy the same height whether it sits at 80 Hz or at 400 Hz. On a linear
   * axis a low voice would be squashed into the bottom of the panel.
   *
   * Drawn as separate strokes per voiced run — connecting across an unvoiced gap
   * would draw a glide the speaker never made.
   */
  private drawPitch(
    ctx: CanvasRenderingContext2D,
    vp: Voiceprint,
    x: (f: number) => number,
    top: number,
    height: number,
    colour: string,
  ): void {
    const logMin = Math.log2(PITCH_MIN_HZ);
    const span = Math.log2(PITCH_MAX_HZ) - logMin;
    const y = (hz: number): number => top + height - ((Math.log2(hz) - logMin) / span) * height;

    ctx.strokeStyle = colour;
    ctx.lineWidth = 1.75;
    ctx.lineJoin = "round";

    let drawing = false;
    ctx.beginPath();
    vp.pitch.hz.forEach((hz, i) => {
      if (hz === null) {
        drawing = false;
        return;
      }
      if (drawing) ctx.lineTo(x(i), y(hz));
      else ctx.moveTo(x(i), y(hz));
      drawing = true;
    });
    ctx.stroke();
  }

  /** Level as a filled area — the shape of the phrasing, not individual values. */
  private drawLevel(
    ctx: CanvasRenderingContext2D,
    vp: Voiceprint,
    x: (f: number) => number,
    top: number,
    height: number,
    colour: string,
  ): void {
    const span = LEVEL_CEILING_DB - LEVEL_FLOOR_DB;
    const y = (db: number): number =>
      top + height - (Math.max(0, Math.min(1, (db - LEVEL_FLOOR_DB) / span)) * height);

    ctx.fillStyle = colour;
    ctx.globalAlpha = 0.35;
    ctx.beginPath();
    ctx.moveTo(0, top + height);
    vp.rmsDb.forEach((db, i) => {
      ctx.lineTo(x(i), y(db));
    });
    ctx.lineTo(x(vp.rmsDb.length - 1), top + height);
    ctx.closePath();
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  /** Flux curve with a tick at every picked onset. */
  private drawFlux(
    ctx: CanvasRenderingContext2D,
    vp: Voiceprint,
    x: (f: number) => number,
    top: number,
    height: number,
    colour: string,
    onsetColour: string,
  ): void {
    ctx.strokeStyle = colour;
    ctx.globalAlpha = 0.5;
    ctx.lineWidth = 1;
    ctx.beginPath();
    vp.events.flux.forEach((v, i) => {
      const y = top + height - v * height;
      if (i === 0) ctx.moveTo(x(i), y);
      else ctx.lineTo(x(i), y);
    });
    ctx.stroke();
    ctx.globalAlpha = 1;

    ctx.strokeStyle = onsetColour;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    for (const frame of vp.events.onsetFrames) {
      ctx.moveTo(x(frame), top);
      ctx.lineTo(x(frame), top + height);
    }
    ctx.stroke();
  }
}

/** Room above each panel for its caption. */
const LABEL_HEIGHT = 16;
/** Gap below each panel, where its baseline is drawn. */
const PANEL_GAP = 8;
