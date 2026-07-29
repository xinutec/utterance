import { DecimalPipe } from "@angular/common";
import { ChangeDetectionStrategy, Component, OnInit, computed, inject, signal } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatCardModule } from "@angular/material/card";
import { MatIconModule } from "@angular/material/icon";
import { MatProgressBarModule } from "@angular/material/progress-bar";

import { Recorder } from "../../audio/recorder";
import { RecordingsStore } from "../../recordings-store";
import { STEPS, assess } from "./steps";

/**
 * The guided calibration.
 *
 * Walks one step at a time, records one take per step, and says whether what
 * came out is usable — see `steps.ts` for why it is a take per step rather than
 * prompts inside one recording.
 *
 * Nothing here blocks. A step can be skipped, a poor take can be kept, and the
 * order is a suggestion: the person at the microphone knows things the checks do
 * not, like whether a van went past.
 */
@Component({
  selector: "app-calibration",
  templateUrl: "./calibration.html",
  styleUrl: "./calibration.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DecimalPipe, MatButtonModule, MatCardModule, MatIconModule, MatProgressBarModule],
})
export class Calibration implements OnInit {
  readonly store = inject(RecordingsStore);
  readonly recorder = inject(Recorder);

  readonly steps = STEPS;
  readonly captureSupported = Recorder.supported;
  readonly captureError = signal<string | null>(null);

  readonly index = signal(0);
  readonly step = computed(() => STEPS[this.index()]);
  readonly isLast = computed(() => this.index() === STEPS.length - 1);

  /** Step ids that already have at least one take, so the list can show progress. */
  readonly recorded = computed(() => new Set(this.store.recordings().map((r) => r.label)));

  /**
   * The verdict on the current step's take.
   *
   * Derived from whatever take is open rather than remembered from the upload,
   * so it survives navigating away and back, and so re-recording a step
   * replaces the verdict without any bookkeeping. Takes are labelled with the
   * step id, which is what ties the two together.
   */
  readonly verdict = computed(() => {
    const detail = this.store.selected();
    if (detail?.meta.label !== this.step().id) return null;
    return assess(this.step(), detail);
  });

  ngOnInit(): void {
    this.store.refresh();
  }

  go(index: number): void {
    this.index.set(Math.max(0, Math.min(STEPS.length - 1, index)));
  }

  async startRecording(): Promise<void> {
    this.captureError.set(null);
    this.store.clearError();
    try {
      await this.recorder.start();
    } catch (err: unknown) {
      this.captureError.set(
        err instanceof DOMException && err.name === "NotAllowedError"
          ? "microphone access was refused — allow it in the browser's site settings and try again"
          : err instanceof Error
            ? err.message
            : String(err),
      );
    }
  }

  async stopRecording(): Promise<void> {
    const take = await this.recorder.stop();
    if (!take) {
      this.captureError.set("nothing was captured — is the right input device selected?");
      return;
    }
    // Labelled with the step id so the audio says what it was for. The whole
    // take carries the label, which is the only claim about it that is exact.
    // Declared as calibration: these guided vowels are what the scale,
    // the timbre, the pitch range and the vowel space are derived from.
    // Everything else uploaded here is material to render and must not
    // shape the speaker — see `Role` in the store.
    this.store.upload(take.wav, this.step().id, "calibration");
  }

  async cancelRecording(): Promise<void> {
    await this.recorder.cancel();
  }
}
