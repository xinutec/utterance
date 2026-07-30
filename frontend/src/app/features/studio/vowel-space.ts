import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnDestroy,
  effect,
  input,
  viewChild,
} from "@angular/core";

import type { SpeakerCorner, Voiceprint } from "../../models";
import { onColourSchemeChange, resolveThemeColours } from "./theme-colours";

/**
 * Axis bounds, in Hz.
 *
 * Deliberately identical to the anatomical ranges the analyser will report a
 * formant in (`formant::RANGES`). Plotting a narrower window than the
 * measurement would silently drop real estimates off the edge of the picture,
 * and nothing on screen would say a point was missing.
 */
const F1_RANGE = { min: 200, max: 1100 };
const F2_RANGE = { min: 600, max: 3000 };

/**
 * Cardinal vowel positions, for orientation only.
 *
 * Approximate values for an adult speaker — a reference grid to read the plot
 * against, not a claim about where this speaker's vowels ought to sit. Every
 * vocal tract is a different size, so absolute positions shift; the *shape* is
 * what transfers.
 *
 * **Shown only until the speaker's own corners exist**, and labelled as generic
 * while it is. Once the guided vowels have been recorded these are replaced by
 * measurements of this mouth, because the two are numbers of the same kind: a
 * dot at 280/2250 looks identical whether it came from a population table or
 * from the person at the microphone, and only one of them is about them.
 */
const LANDMARKS = [
  { label: "i (beet)", f1: 280, f2: 2250 },
  { label: "ɛ (bet)", f1: 530, f2: 1840 },
  { label: "a (father)", f1: 730, f2: 1100 },
  { label: "ɔ (bought)", f1: 570, f2: 840 },
  { label: "u (boot)", f1: 300, f2: 870 },
] as const;

/**
 * What to call each measured corner on the chart.
 *
 * The sound the person was asked for, not the corner's technical name: they were
 * told to say "ee", and "ee" is what makes the point on the picture recognisable
 * as the thing they did.
 */
const CORNER_LABELS: Record<SpeakerCorner["corner"], string> = {
  closeFront: "ee",
  open: "ah",
  closeBack: "oo",
};

/**
 * The speaker's path through vowel space.
 *
 * Plotted the way phoneticians plot it — F2 decreasing left to right, F1
 * increasing downward — so the picture lines up with the IPA vowel
 * quadrilateral: close vowels at the top, front vowels on the left. That is not
 * decoration. It means the plot can be read directly as tongue position, which
 * is what makes an unfamiliar trajectory interpretable at a glance.
 *
 * Colour runs from the start of the take to the end, so the direction of travel
 * is visible in a still image.
 */
@Component({
  selector: "app-vowel-space",
  templateUrl: "./vowel-space.html",
  styleUrl: "./vowel-space.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class VowelSpace implements AfterViewInit, OnDestroy {
  readonly voiceprint = input.required<Voiceprint>();

  /**
   * This speaker's own corners, from the guided vowels.
   *
   * Empty until those are recorded, and empty is not a failure — it is the state
   * of a store whose owner has not done the calibration yet, and the plot says
   * so rather than passing population values off as theirs.
   */
  readonly corners = input<readonly SpeakerCorner[]>([]);

  private readonly canvasRef = viewChild.required<ElementRef<HTMLCanvasElement>>("canvas");
  private observer?: ResizeObserver;
  private stopWatchingScheme?: () => void;

  constructor() {
    effect(() => {
      const vp = this.voiceprint();
      // Read so the effect re-runs when the corners arrive: they are fetched
      // separately from the take, so they land after the first draw.
      this.corners();
      if (this.observer) this.draw(vp);
    });
  }

  ngAfterViewInit(): void {
    this.observer = new ResizeObserver(() => {
      this.draw(this.voiceprint());
    });
    this.observer.observe(this.canvasRef().nativeElement);
    this.stopWatchingScheme = onColourSchemeChange(() => {
      this.draw(this.voiceprint());
    });
    this.draw(this.voiceprint());
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
    this.stopWatchingScheme?.();
  }

  private draw(vp: Voiceprint): void {
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

    const { ink, muted } = resolveThemeColours(canvas.parentElement ?? canvas);

    const pad = { top: 18, right: 14, bottom: 28, left: 44 };
    const plot = {
      width: width - pad.left - pad.right,
      height: height - pad.top - pad.bottom,
    };

    // Both axes inverted, per the convention described on the class.
    const x = (f2: number): number =>
      pad.left + ((F2_RANGE.max - f2) / (F2_RANGE.max - F2_RANGE.min)) * plot.width;
    const y = (f1: number): number =>
      pad.top + ((f1 - F1_RANGE.min) / (F1_RANGE.max - F1_RANGE.min)) * plot.height;

    this.drawAxes(ctx, muted, x, y, pad, plot);
    const corners = this.corners();
    if (corners.length > 0) {
      this.drawCorners(ctx, ink, x, y, corners);
    } else {
      this.drawLandmarks(ctx, muted, x, y);
    }

    // The trajectory, oldest to newest.
    const points = this.positions(vp);
    for (const [index, point] of points.entries()) {
      const progress = points.length > 1 ? index / (points.length - 1) : 0;
      ctx.fillStyle = `hsl(${210 + progress * 120} 80% ${55 + progress * 10}%)`;
      ctx.globalAlpha = 0.55;
      ctx.beginPath();
      ctx.arc(x(point.f2), y(point.f1), 2.4, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;

    if (points.length === 0) {
      ctx.fillStyle = ink;
      ctx.font = "13px system-ui, sans-serif";
      ctx.fillText("no voiced frames with both formants", pad.left, pad.top + plot.height / 2);
    }
  }

  /**
   * Frames with both formants.
   *
   * Out-of-range points are excluded, not clamped — clamping would pile them
   * against an axis and invent a cluster the speaker never produced. Given the
   * bounds match the analyser's, this should exclude nothing in practice.
   */
  private positions(vp: Voiceprint): { f1: number; f2: number }[] {
    const out: { f1: number; f2: number }[] = [];
    for (let i = 0; i < vp.frame.count; i++) {
      const f1 = vp.formants.f1[i];
      const f2 = vp.formants.f2[i];
      if (f1 === null || f2 === null) continue;
      if (f1 < F1_RANGE.min || f1 > F1_RANGE.max) continue;
      if (f2 < F2_RANGE.min || f2 > F2_RANGE.max) continue;
      out.push({ f1, f2 });
    }
    return out;
  }

  private drawAxes(
    ctx: CanvasRenderingContext2D,
    muted: string,
    x: (f2: number) => number,
    y: (f1: number) => number,
    pad: { top: number; left: number },
    plot: { width: number; height: number },
  ): void {
    ctx.strokeStyle = muted;
    ctx.fillStyle = muted;
    ctx.font = "10px system-ui, sans-serif";
    ctx.globalAlpha = 0.5;
    ctx.strokeRect(pad.left, pad.top, plot.width, plot.height);
    ctx.globalAlpha = 1;

    for (const f2 of [2400, 2000, 1600, 1200, 800]) {
      ctx.fillText(String(f2), x(f2) - 12, pad.top + plot.height + 14);
    }
    for (const f1 of [300, 500, 700, 900]) {
      ctx.fillText(String(f1), 6, y(f1) + 3);
    }
    ctx.fillText("F2 (Hz) →  back", pad.left, pad.top + plot.height + 26);
    ctx.save();
    ctx.translate(12, pad.top + plot.height / 2);
    ctx.rotate(-Math.PI / 2);
    ctx.fillText("F1 (Hz) →  open", -30, -22);
    ctx.restore();
  }

  private drawLandmarks(
    ctx: CanvasRenderingContext2D,
    muted: string,
    x: (f2: number) => number,
    y: (f1: number) => number,
  ): void {
    ctx.fillStyle = muted;
    ctx.font = "11px system-ui, sans-serif";
    ctx.globalAlpha = 0.65;
    for (const mark of LANDMARKS) {
      ctx.fillText(mark.label, x(mark.f2) + 4, y(mark.f1));
      ctx.beginPath();
      ctx.arc(x(mark.f2), y(mark.f1), 2, 0, Math.PI * 2);
      ctx.fill();
    }
    // Said once, because the difference between these and the speaker's own is
    // invisible on the picture and decides how much the picture is worth.
    ctx.fillText("typical adult positions — record the guided vowels for yours", x(F2_RANGE.max) + 4, y(F1_RANGE.max) - 4);
    ctx.globalAlpha = 1;
  }

  /**
   * This speaker's measured corners, with the spread they were held to.
   *
   * The cross is the interquartile range on each axis, so a vowel that wandered
   * is drawn as the region it wandered through rather than as a dot. A dot would
   * claim the same precision for a held take and a smeared one, and the smeared
   * one is exactly the case someone needs to see — it means the corner is soft
   * and everything normalised against it inherits that.
   *
   * Drawn in the trajectory's own ink rather than muted: these are measurements
   * of this person, not a reference grid to read them against.
   */
  private drawCorners(
    ctx: CanvasRenderingContext2D,
    ink: string,
    x: (f2: number) => number,
    y: (f1: number) => number,
    corners: readonly SpeakerCorner[],
  ): void {
    ctx.strokeStyle = ink;
    ctx.fillStyle = ink;
    ctx.font = "11px system-ui, sans-serif";
    ctx.lineWidth = 1;

    for (const corner of corners) {
      const cx = x(corner.f2Hz);
      const cy = y(corner.f1Hz);

      // Half an interquartile range either side of centre, which is what the
      // quartiles bound. Full-width arms would draw twice the spread measured.
      const halfF2 = Math.abs(x(corner.f2Hz + corner.f2SpreadHz / 2) - cx);
      const halfF1 = Math.abs(y(corner.f1Hz + corner.f1SpreadHz / 2) - cy);

      ctx.globalAlpha = 0.35;
      ctx.beginPath();
      ctx.moveTo(cx - halfF2, cy);
      ctx.lineTo(cx + halfF2, cy);
      ctx.moveTo(cx, cy - halfF1);
      ctx.lineTo(cx, cy + halfF1);
      ctx.stroke();

      ctx.globalAlpha = 0.9;
      ctx.beginPath();
      ctx.arc(cx, cy, 3.5, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillText(CORNER_LABELS[corner.corner], cx + 6, cy - 5);
    }
    ctx.globalAlpha = 1;
  }
}
