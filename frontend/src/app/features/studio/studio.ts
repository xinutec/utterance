import { DecimalPipe } from "@angular/common";
import { ChangeDetectionStrategy, Component, OnInit, inject, signal } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatCardModule } from "@angular/material/card";
import { MatIconModule } from "@angular/material/icon";
import { MatProgressBarModule } from "@angular/material/progress-bar";
import { MatTooltipModule } from "@angular/material/tooltip";

import { Recorder } from "../../audio/recorder";
import type { RecordingMeta } from "../../models";
import { RecordingsStore } from "../../recordings-store";
import { VoiceprintChart } from "./voiceprint-chart";

/** Target take length, in seconds. Not enforced — just what the UI suggests. */
const TARGET_SECONDS = 30;

/** Peak level above which a take is likely clipped and worth re-recording. */
const CLIPPING_PEAK = 0.99;

@Component({
  selector: "app-studio",
  templateUrl: "./studio.html",
  styleUrl: "./studio.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    DecimalPipe,
    MatButtonModule,
    MatCardModule,
    MatIconModule,
    MatProgressBarModule,
    MatTooltipModule,
    VoiceprintChart,
  ],
})
export class Studio implements OnInit {
  readonly store = inject(RecordingsStore);
  readonly recorder = inject(Recorder);

  /** Capture problems, which are this component's own — not the store's. */
  readonly captureError = signal<string | null>(null);
  readonly warning = signal<string | null>(null);

  readonly targetSeconds = TARGET_SECONDS;
  readonly captureSupported = Recorder.supported;

  ngOnInit(): void {
    this.store.refresh();
  }

  async startRecording(): Promise<void> {
    this.captureError.set(null);
    this.warning.set(null);
    this.store.clearError();
    try {
      await this.recorder.start();
    } catch (err: unknown) {
      // Overwhelmingly this is a denied permission prompt, which is a thing the
      // person can fix — so name it rather than reporting a raw DOMException.
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
    if (take.peak >= CLIPPING_PEAK) {
      this.warning.set("that take reached full scale and is probably clipped — worth recording again quieter");
    }
    this.store.upload(take.wav, `take ${new Date().toLocaleTimeString()}`);
  }

  async cancelRecording(): Promise<void> {
    await this.recorder.cancel();
  }

  onFileChosen(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) this.store.upload(file, file.name);
    // Clear it, so choosing the same file twice in a row still fires a change.
    input.value = "";
  }

  remove(meta: RecordingMeta, event: MouseEvent): void {
    event.stopPropagation();
    this.store.remove(meta);
  }
}
