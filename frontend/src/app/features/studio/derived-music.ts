import { DecimalPipe } from "@angular/common";
import {
  ChangeDetectionStrategy,
  Component,
  type OnInit,
  computed,
  inject,
  input,
  signal,
} from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";
import { MatProgressBarModule } from "@angular/material/progress-bar";

import { ControlsStore } from "../../controls-store";
import type { ScaleDegree, VoiceSummary } from "../../models";
import { ApiError, RecordingsApi, UNEXPLAINED } from "../../recordings-api";
import { MappingControls } from "./mapping-controls";
import { INITIAL_SETTINGS, settingsQuery, type MappingSettings } from "./mapping-settings";

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
 * What this take sounds like as music, with the scale beside the player: that
 * the intervals came from the speaker's spectrum is easier to believe from a
 * number than from a listen.
 */
@Component({
  selector: "app-derived-music",
  templateUrl: "./derived-music.html",
  styleUrl: "./derived-music.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    DecimalPipe,
    MatButtonModule,
    MatIconModule,
    MatProgressBarModule,
    MappingControls,
  ],
})
export class DerivedMusic implements OnInit {
  private readonly api = inject(RecordingsApi);

  /** The take to render. */
  readonly recordingId = input.required<string>();

  /**
   * What the next render will be made with — held here because the scale shown
   * depends on it too (`bind` and `calibration`).
   */
  readonly settings = signal<MappingSettings>(INITIAL_SETTINGS);

  /** What the mapping accepts. Held app-wide: it cannot change while open. */
  private readonly controls = inject(ControlsStore);
  readonly knobs = this.controls.knobs;
  readonly mappings = this.controls.mappings;

  /** The query the current settings imply, shared by both requests. */
  private readonly query = computed(() => settingsQuery(this.settings(), this.knobs()));

  /** The speaker's scale, kept across takes: it is about the person. */
  readonly voice = signal<VoiceSummary | null>(null);
  readonly error = signal<string | null>(null);
  readonly loading = signal(false);

  /** The take and the URL the person last asked to hear, exactly as asked. */
  private readonly rendered = signal<{ id: string; url: string } | null>(null);

  /**
   * Where the player points, or `null` until asked — keyed on the take, so
   * choosing another take clears it. Renders are seconds of work, so they start
   * only on request.
   */
  readonly renderUrl = computed(() => {
    const rendered = this.rendered();
    return rendered?.id === this.recordingId() ? rendered.url : null;
  });

  /**
   * True when the settings have moved since what is playing was made. The old
   * audio stays, so a knob can be moved while listening.
   */
  readonly stale = computed(() => {
    const url = this.renderUrl();
    return url !== null && url !== this.api.renderUrl(this.recordingId(), this.query());
  });

  /**
   * Why the chosen mapping cannot be played in this scale, if it cannot — said
   * here, since a refused render reaches the player as a broken control.
   */
  readonly refusal = computed(() => this.voice()?.refusal ?? null);



  readonly degrees = computed<ShownDegree[]>(() => {
    const summary = this.voice();
    if (!summary) return [];
    const deepest = Math.max(...summary.degrees.map((d: ScaleDegree) => d.depth), 0);
    return summary.degrees.map((d: ScaleDegree) => ({
      cents: d.cents,
      ratio: d.ratio,
      offEqual: d.cents - Math.round(d.cents / SEMITONE_CENTS) * SEMITONE_CENTS,
      // Endpoints have no depth; give the tonic and octave a full bar.
      weight: d.depth === 0 ? 1 : (deepest > 0 ? d.depth / deepest : 0),
    }));
  });

  ngOnInit(): void {
    this.controls.ensure();
  }

  /**
   * Fetch the scale and point the player at a render, together, so the degrees
   * shown are the degrees that sound.
   */
  load(): void {
    this.loading.set(true);
    this.error.set(null);
    const id = this.recordingId();
    const query = this.query();
    this.api.voice(query).subscribe({
      next: (summary) => {
        this.voice.set(summary);
        // Refused: leave the current audio playing rather than a broken player.
        if (!summary.refusal) {
          this.rendered.set({ id, url: this.api.renderUrl(id, query) });
        }
        this.loading.set(false);
      },
      error: (err: unknown) => {
        this.loading.set(false);
        this.error.set(err instanceof ApiError ? err.message : UNEXPLAINED);
      },
    });
  }
}
