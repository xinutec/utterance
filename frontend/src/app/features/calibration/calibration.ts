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
 * The guided calibration: one take per step (see `steps.ts`), each checked for
 * usability. Nothing blocks — the person at the microphone knows things the
 * checks do not, like whether a van went past.
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
  /**
   * The step being shown, falling back to the first: `index` only moves within
   * bounds, so this beats guarding every binding.
   */
  readonly step = computed(() => STEPS[this.index()] ?? STEPS[0]);
  readonly isLast = computed(() => this.index() === STEPS.length - 1);

  /** Step ids that already have at least one take, so the list can show progress. */
  readonly recorded = computed(() => new Set(this.store.recordings().map((r) => r.label)));

  /**
   * The verdict on the current step's take, derived from the open take (matched
   * by label), so it survives navigation and re-recording.
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
            // Never `String(err)`, which reads "[object Object]".
            : "the microphone could not be opened",
      );
    }
  }

  async stopRecording(): Promise<void> {
    const take = await this.recorder.stop();
    if (!take) {
      this.captureError.set("nothing was captured — is the right input device selected?");
      return;
    }
    // Labelled with the step id, and declared calibration: these takes define
    // the speaker; everything else uploaded is material.
    this.store.upload(take.wav, this.step().id, "calibration");
  }

  async cancelRecording(): Promise<void> {
    await this.recorder.cancel();
  }
}
