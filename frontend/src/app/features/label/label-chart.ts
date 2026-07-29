import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  afterNextRender,
  effect,
  input,
  model,
  output,
  signal,
  viewChild,
} from "@angular/core";

import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";

import type { Syllable, Voiceprint } from "../../models";

/** Quietest level drawn, in dBFS. Below this a frame is silence to the eye. */
const FLOOR_DB = -70;

/** How close a click must be to a mark to mean *that* mark, in pixels. */
const GRAB_PX = 8;

/**
 * Pixels per second at each zoom step.
 *
 * **The reason this component exists at all.** A 46-second take across a phone
 * is under 9 px/second, and a syllable at four a second is 230 ms — *two
 * pixels*. Nothing can be placed, seen or dragged at that scale, so the view has
 * to magnify and pan. The top of the range puts a syllable at ~46 px, which is
 * enough to put its edge within a few tens of milliseconds by eye.
 */
const ZOOMS = [25, 50, 100, 200, 400];

/**
 * The take's loudness over time, with the marks on it.
 *
 * **Level, deliberately, and not the flux curve.** Flux is what the onset
 * detector reads, and these marks exist to judge that detector — showing it
 * would pull every mark toward what it already believes, which is the same
 * objection as pre-placing marks from it, in a weaker and less obvious form.
 * Level is a different measurement and roughly what the ear tracks.
 *
 * Canvas rather than SVG: a few thousand frames is a few thousand DOM nodes, and
 * the page stops being usable long before the take stops being ordinary.
 */
@Component({
  selector: "app-label-chart",
  templateUrl: "./label-chart.html",
  styleUrl: "./label-chart.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MatButtonModule, MatIconModule],
})
export class LabelChart {
  readonly voiceprint = input.required<Voiceprint>();
  readonly syllables = model.required<readonly Syllable[]>();
  /** Where the player is, in seconds. */
  readonly playhead = input(0);
  /** Asked for when somebody clicks the chart's background. */
  readonly seek = output<number>();

  readonly zoom = signal(2);
  private readonly canvasRef = viewChild.required<ElementRef<HTMLCanvasElement>>("canvas");
  private readonly scrollRef = viewChild.required<ElementRef<HTMLElement>>("scroller");

  /** Index of the mark being dragged, or null. */
  private dragging: number | null = null;

  constructor() {
    // Nothing can be measured before the first render, and every input change
    // after it has to repaint.
    afterNextRender(() => this.draw());
    effect(() => {
      this.voiceprint();
      this.syllables();
      this.playhead();
      this.zoom();
      this.offset();
      this.draw();
    });
  }

  get pixelsPerSecond(): number {
    return ZOOMS[this.zoom()];
  }

  /** Total width the take needs at the current magnification. */
  get width(): number {
    return Math.max(1, Math.ceil(this.duration * this.pixelsPerSecond));
  }

  private get duration(): number {
    const vp = this.voiceprint();
    return vp.frame.count * vp.frame.hopS;
  }

  zoomIn(): void {
    this.zoom.update((z) => Math.min(ZOOMS.length - 1, z + 1));
  }

  zoomOut(): void {
    this.zoom.update((z) => Math.max(0, z - 1));
  }

  /** How far the window has been scrolled, in seconds. */
  readonly offset = signal(0);

  onScroll(): void {
    this.offset.set(this.scrollRef().nativeElement.scrollLeft / this.pixelsPerSecond);
  }

  /** Seconds at a pixel offset within the canvas, allowing for the scroll. */
  private secondsAt(clientX: number): number {
    const box = this.canvasRef().nativeElement.getBoundingClientRect();
    const seconds = this.offset() + (clientX - box.left) / this.pixelsPerSecond;
    return Math.max(0, Math.min(this.duration, seconds));
  }

  /** The mark nearest a position, if one is close enough to have been meant. */
  private markAt(seconds: number): number | null {
    const marks = this.syllables();
    let best: { index: number; distance: number } | null = null;
    marks.forEach((mark, index) => {
      const distance = Math.abs(mark.atS - seconds) * this.pixelsPerSecond;
      if (distance <= GRAB_PX && (!best || distance < best.distance)) {
        best = { index, distance };
      }
    });
    return best === null ? null : (best as { index: number }).index;
  }

  onPointerDown(event: PointerEvent): void {
    const at = this.secondsAt(event.clientX);
    const existing = this.markAt(at);
    if (existing !== null) {
      // Grab it. Whether this turns out to be a drag or a click to remove is
      // decided on release, by whether it moved.
      this.dragging = existing;
      this.movedWhileDragging = false;
      this.canvasRef().nativeElement.setPointerCapture(event.pointerId);
      return;
    }
    // Empty ground: a new mark, and the playhead follows so it can be heard.
    this.syllables.update((marks) => [...marks, { atS: at }].sort((a, b) => a.atS - b.atS));
    this.seek.emit(at);
  }

  private movedWhileDragging = false;

  onPointerMove(event: PointerEvent): void {
    if (this.dragging === null) return;
    const at = this.secondsAt(event.clientX);
    this.movedWhileDragging = true;
    this.syllables.update((marks) =>
      marks.map((mark, i) => (i === this.dragging ? { atS: at } : mark)),
    );
  }

  onPointerUp(event: PointerEvent): void {
    if (this.dragging === null) return;
    const index = this.dragging;
    this.dragging = null;
    this.canvasRef().nativeElement.releasePointerCapture(event.pointerId);

    if (this.movedWhileDragging) {
      // A drag can carry a mark past its neighbour; the order is the store's
      // business, but the screen must not show it out of sequence in between.
      this.syllables.update((marks) => [...marks].sort((a, b) => a.atS - b.atS));
      return;
    }
    // Pressed and released without moving: remove it. Placing and removing are
    // the two operations, and both are one gesture on the thing itself.
    this.syllables.update((marks) => marks.filter((_, i) => i !== index));
  }

  private draw(): void {
    const canvas = this.canvasRef().nativeElement;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const vp = this.voiceprint();
    const ratio = window.devicePixelRatio || 1;
    // The window in view, not the whole take.
    const width = canvas.clientWidth || 1;
    const height = canvas.clientHeight || 120;
    canvas.width = Math.floor(width * ratio);
    canvas.height = Math.floor(height * ratio);
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, width, height);

    const style = getComputedStyle(canvas);
    const ink = style.getPropertyValue("--chart-ink").trim() || "#888";
    const markInk = style.getPropertyValue("--chart-mark").trim() || "#c00";
    const headInk = style.getPropertyValue("--chart-head").trim() || "#06c";

    // Level, as an area. One point per frame; at the widest zoom that is four
    // pixels a frame, which is why the edges of a syllable are placeable.
    const from = this.offset();
    const x = (seconds: number) => (seconds - from) * this.pixelsPerSecond;
    const y = (db: number) =>
      height - Math.max(0, Math.min(1, (db - FLOOR_DB) / -FLOOR_DB)) * height;

    ctx.fillStyle = ink;
    ctx.globalAlpha = 0.35;
    ctx.beginPath();
    ctx.moveTo(0, height);
    // Only the frames the window covers: a minute of speech is six thousand
    // frames, and drawing them all to paint four hundred pixels is wasted work
    // on every scroll step.
    const first = Math.max(0, Math.floor(from / vp.frame.hopS) - 1);
    const last = Math.min(
      vp.rmsDb.length,
      Math.ceil((from + width / this.pixelsPerSecond) / vp.frame.hopS) + 1,
    );
    ctx.moveTo(x(first * vp.frame.hopS), height);
    for (let i = first; i < last; i++) ctx.lineTo(x(i * vp.frame.hopS), y(vp.rmsDb[i]));
    ctx.lineTo(x((last - 1) * vp.frame.hopS), height);
    ctx.closePath();
    ctx.fill();
    ctx.globalAlpha = 1;

    // Marks.
    ctx.strokeStyle = markInk;
    ctx.lineWidth = 2;
    for (const mark of this.syllables()) {
      const at = x(mark.atS);
      if (at < -2 || at > width + 2) continue;
      ctx.beginPath();
      ctx.moveTo(at, 0);
      ctx.lineTo(at, height);
      ctx.stroke();
    }

    // Playhead, drawn last so it is never hidden by a mark it sits on.
    ctx.strokeStyle = headInk;
    ctx.lineWidth = 1;
    const head = x(this.playhead());
    ctx.beginPath();
    ctx.moveTo(head, 0);
    ctx.lineTo(head, height);
    ctx.stroke();

    // Keep the playhead in view while the take plays, so a long take does not
    // scroll away from whoever is listening to it.
    const scroller = this.scrollRef().nativeElement;
    if (head < 0 || head > width) {
      scroller.scrollLeft = Math.max(
        0,
        this.playhead() * this.pixelsPerSecond - scroller.clientWidth / 2,
      );
    }
  }
}
