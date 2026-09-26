import { DecimalPipe } from "@angular/common";
import { ChangeDetectionStrategy, Component, OnInit, computed, inject, signal } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatCardModule } from "@angular/material/card";
import { MatIconModule } from "@angular/material/icon";
import { MatProgressBarModule } from "@angular/material/progress-bar";
import { MatTooltipModule } from "@angular/material/tooltip";

import { RouterLink } from "@angular/router";

import { Recorder } from "../../audio/recorder";
import { Help } from "../../help";
import type { RecordingMeta } from "../../models";
import { RecordingsStore } from "../../recordings-store";
import { DerivedMusic } from "./derived-music";
import { VoiceprintChart } from "./voiceprint-chart";
import { VowelSpace } from "./vowel-space";

/** Target take length, in seconds. Not enforced — just what the UI suggests. */
const TARGET_SECONDS = 30;

@Component({
  selector: "app-studio",
  templateUrl: "./studio.html",
  styleUrl: "./studio.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    DecimalPipe,
    Help,
    MatButtonModule,
    RouterLink,
    MatCardModule,
    MatIconModule,
    MatProgressBarModule,
    MatTooltipModule,
    DerivedMusic,
    VoiceprintChart,
    VowelSpace,
  ],
})
export class Studio implements OnInit {
  readonly store = inject(RecordingsStore);
  readonly recorder = inject(Recorder);

  /**
   * True while no take says who the speaker is — read from the take list, so
   * the page offers the next move before a render is refused.
   */
  readonly needsCalibration = computed(
    () => !this.store.recordings().some((take) => take.role === "calibration"),
  );

  /** Capture problems, which are this component's own — not the store's. */
  readonly captureError = signal<string | null>(null);

  /**
   * Quality warning about the open take, from the analyser's own measurement, so
   * an uploaded file is checked like a recording.
   */
  readonly warning = computed(() => {
    const detail = this.store.selected();
    // The backend decides "clipped"; this only formats the number.
    if (!detail?.meta.clipped) return null;
    const percent = detail.voiceprint.source.clippedFraction * 100;
    return (
      `this take is clipped — ${percent.toFixed(1)}% of it is pinned at full scale. ` +
      `Clipping is distortion, and it corrupts the harmonic amplitudes the tuning is derived from. ` +
      `Worth recording again with the input a few dB lower.`
    );
  });

  readonly targetSeconds = TARGET_SECONDS;
  readonly captureSupported = Recorder.supported;

  ngOnInit(): void {
    this.store.refresh();
  }

  async startRecording(): Promise<void> {
    this.captureError.set(null);
    this.store.clearError();
    try {
      await this.recorder.start();
    } catch (err: unknown) {
      // Usually a denied permission prompt, which the person can fix.
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
    this.store.upload(take.wav, `take ${new Date().toLocaleTimeString()}`);
  }

  async cancelRecording(): Promise<void> {
    await this.recorder.cancel();
  }

  onFileChosen(event: Event): void {
    const input = event.target;
    if (!(input instanceof HTMLInputElement)) return;
    const file = input.files?.[0];
    if (file) this.store.upload(file, file.name);
    // Clear it, so choosing the same file twice in a row still fires a change.
    input.value = "";
  }

  remove(meta: RecordingMeta, event: MouseEvent): void {
    event.stopPropagation();
    this.store.remove(meta);
  }

  /**
   * Turn a take into the voice, or stop it being one — on the row, for takes
   * that never went through the guided steps.
   */
  toggleRole(meta: RecordingMeta, event: MouseEvent): void {
    event.stopPropagation();
    this.store.setRole(meta, meta.role === "calibration" ? "material" : "calibration");
  }
}
