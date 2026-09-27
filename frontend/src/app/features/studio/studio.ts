import { DecimalPipe } from "@angular/common";
import { ChangeDetectionStrategy, Component, OnInit, computed, inject, signal } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatCardModule } from "@angular/material/card";
import { MatIconModule } from "@angular/material/icon";
import { MatProgressBarModule } from "@angular/material/progress-bar";
import { MatTooltipModule } from "@angular/material/tooltip";

import { Router, RouterLink } from "@angular/router";

import { Recorder } from "../../audio/recorder";
import { Help } from "../../help";
import type { RecordingMeta } from "../../models";
import { RecordingsStore } from "../../recordings-store";

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
  ],
})
export class Studio implements OnInit {
  readonly store = inject(RecordingsStore);
  readonly recorder = inject(Recorder);
  private readonly router = inject(Router);

  /** Open a take just stored: it is what the person came to see. */
  private readonly goTo = (id: string): void => {
    void this.router.navigate(["/take", id]);
  };

  /**
   * True while no take says who the speaker is — read from the take list, so
   * the page offers the next move before a render is refused.
   */
  readonly needsCalibration = computed(
    () => !this.store.recordings().some((take) => take.role === "calibration"),
  );

  /** Capture problems, which are this component's own — not the store's. */
  readonly captureError = signal<string | null>(null);

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
    this.store.upload(take.wav, `take ${new Date().toLocaleTimeString()}`, "material", this.goTo);
  }

  async cancelRecording(): Promise<void> {
    await this.recorder.cancel();
  }

  onFileChosen(event: Event): void {
    const input = event.target;
    if (!(input instanceof HTMLInputElement)) return;
    const file = input.files?.[0];
    if (file) this.store.upload(file, file.name, "material", this.goTo);
    // Clear it, so choosing the same file twice in a row still fires a change.
    input.value = "";
  }

  /**
   * Turn a take into the voice, or stop it being one — on the row, for takes
   * that never went through the guided steps.
   */
  toggleRole(meta: RecordingMeta): void {
    this.store.setRole(meta, meta.role === "calibration" ? "material" : "calibration");
  }
}
