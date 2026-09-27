import { DecimalPipe } from "@angular/common";
import { ChangeDetectionStrategy, Component, type OnInit, computed, effect, inject, input } from "@angular/core";
import { MatCardModule } from "@angular/material/card";
import { MatProgressBarModule } from "@angular/material/progress-bar";
import { scaffoldTitle } from "@xinutec/ui-scaffold";

import { RecordingsStore } from "../../recordings-store";
import { DerivedMusic } from "../studio/derived-music";
import { VoiceprintChart } from "../studio/voiceprint-chart";
import { VowelSpace } from "../studio/vowel-space";

/** One take: its voiceprint, the recording, and the music derived from it. */
@Component({
  selector: "app-take",
  templateUrl: "./take.html",
  styleUrl: "./take.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DecimalPipe, MatCardModule, MatProgressBarModule, DerivedMusic, VoiceprintChart, VowelSpace],
})
export class Take implements OnInit {
  readonly store = inject(RecordingsStore);

  /** From the route, `take/:id`. */
  readonly id = input.required<string>();

  /** This take once loaded; the store's open take may still be the last one. */
  readonly detail = computed(() => {
    const open = this.store.selected();
    return open?.meta.id === this.id() ? open : null;
  });

  /**
   * Quality warning from the analyser's own measurement, so an uploaded file is
   * checked like a recording.
   */
  readonly warning = computed(() => {
    const detail = this.detail();
    // The backend decides "clipped"; this only formats the number.
    if (!detail?.meta.clipped) return null;
    const percent = detail.voiceprint.source.clippedFraction * 100;
    return (
      `this take is clipped — ${percent.toFixed(1)}% of it is pinned at full scale. ` +
      `Clipping is distortion, and it corrupts the harmonic amplitudes the tuning is derived from. ` +
      `Worth recording again with the input a few dB lower.`
    );
  });

  constructor() {
    scaffoldTitle(() => this.detail()?.meta.label ?? { text: "Take", provisional: true });
    effect(() => {
      const id = this.id();
      // An upload has opened it already.
      if (this.store.selected()?.meta.id !== id) this.store.open(id);
    });
  }

  ngOnInit(): void {
    // For the speaker's corners, which a take opened by link has not loaded.
    this.store.refresh();
  }
}
