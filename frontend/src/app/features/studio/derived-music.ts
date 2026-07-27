import { DecimalPipe } from "@angular/common";
import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";
import { MatProgressBarModule } from "@angular/material/progress-bar";

import type { ScaleDegree, VoiceSummary } from "../../models";
import { ApiError, RecordingsApi } from "../../recordings-api";

/** Cents of the nearest equal-tempered note, for showing how far off it sits. */
const SEMITONE_CENTS = 100;

/** A degree with the arithmetic a reader wants done for them. */
interface ShownDegree {
  readonly cents: number;
  readonly ratio: number;
  /** Signed distance to the nearest 12-TET note, in cents. */
  readonly offEqual: number;
  /** Depth as a fraction of the deepest degree, for the bar width. */
  readonly weight: number;
}

/**
 * What this take sounds like as music, and the scale it is played in.
 *
 * Deliberately shows the scale beside the player rather than only offering
 * audio. The interesting claim is that these intervals came out of the
 * speaker's own spectrum and are not the twelve everyone else uses, and that is
 * far easier to believe from a number than from a listen.
 */
@Component({
  selector: "app-derived-music",
  templateUrl: "./derived-music.html",
  styleUrl: "./derived-music.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DecimalPipe, MatButtonModule, MatIconModule, MatProgressBarModule],
})
export class DerivedMusic {
  private readonly api = inject(RecordingsApi);

  /** The take to render. */
  readonly recordingId = input.required<string>();

  /**
   * The speaker's scale. Kept across takes on purpose — it is a fact about the
   * person, not about the recording open at the moment.
   */
  readonly voice = signal<VoiceSummary | null>(null);
  readonly error = signal<string | null>(null);
  readonly loading = signal(false);

  /** Which take the person last asked to hear. */
  private readonly rendered = signal<string | null>(null);

  /**
   * Where the player points, or `null` until asked.
   *
   * Derived from the current input rather than stored, so selecting a different
   * take clears the player instead of leaving it offering audio for a recording
   * no longer on screen. Rendering is seconds of backend work, so it starts only
   * when someone asks: otherwise clicking through takes would queue a render for
   * each one.
   */
  readonly renderUrl = computed(() => {
    const id = this.recordingId();
    return this.rendered() === id ? this.api.renderUrl(id) : null;
  });

  readonly degrees = computed<ShownDegree[]>(() => {
    const summary = this.voice();
    if (!summary) return [];
    const deepest = Math.max(...summary.degrees.map((d: ScaleDegree) => d.depth), 0);
    return summary.degrees.map((d: ScaleDegree) => ({
      cents: d.cents,
      ratio: d.ratio,
      offEqual: d.cents - Math.round(d.cents / SEMITONE_CENTS) * SEMITONE_CENTS,
      // Endpoints have no depth by construction; give them a full bar rather
      // than an empty one, since the tonic and octave are the firmest notes
      // there are.
      weight: d.depth === 0 ? 1 : (deepest > 0 ? d.depth / deepest : 0),
    }));
  });

  load(): void {
    this.loading.set(true);
    this.error.set(null);
    this.api.voice().subscribe({
      next: (summary) => {
        this.voice.set(summary);
        this.rendered.set(this.recordingId());
        this.loading.set(false);
      },
      error: (err: unknown) => {
        this.loading.set(false);
        this.error.set(err instanceof ApiError ? err.message : String(err));
      },
    });
  }
}
