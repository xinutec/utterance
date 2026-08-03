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
   * What the next render will be made with.
   *
   * Held here rather than in the controls because two other things depend on
   * it: the render URL, and the scale shown below it — `bind` and `calibration`
   * both change which degrees the backend reports.
   */
  readonly settings = signal<MappingSettings>(INITIAL_SETTINGS);

  /** What the mapping accepts. Held app-wide: it cannot change while open. */
  private readonly controls = inject(ControlsStore);
  readonly knobs = this.controls.knobs;
  readonly mappings = this.controls.mappings;

  /** The query the current settings imply, shared by both requests. */
  private readonly query = computed(() => settingsQuery(this.settings(), this.knobs()));

  /**
   * The speaker's scale. Kept across takes on purpose — it is a fact about the
   * person, not about the recording open at the moment.
   */
  readonly voice = signal<VoiceSummary | null>(null);
  readonly error = signal<string | null>(null);
  readonly loading = signal(false);

  /** The take and the URL the person last asked to hear, exactly as asked. */
  private readonly rendered = signal<{ id: string; url: string } | null>(null);

  /**
   * Where the player points, or `null` until asked.
   *
   * Keyed on the take rather than stored flat, so selecting a different take
   * clears the player instead of leaving it offering audio for a recording no
   * longer on screen. Rendering is seconds of backend work, so it starts only
   * when someone asks: otherwise clicking through takes would queue a render for
   * each one.
   */
  readonly renderUrl = computed(() => {
    const rendered = this.rendered();
    return rendered?.id === this.recordingId() ? rendered.url : null;
  });

  /**
   * True when the settings have moved since what is playing was made.
   *
   * The player keeps the old audio rather than clearing it, so a knob can be
   * moved while listening and the comparison is against something still
   * audible — which is the only way any of these questions get settled.
   */
  readonly stale = computed(() => {
    const url = this.renderUrl();
    return url !== null && url !== this.api.renderUrl(this.recordingId(), this.query());
  });

  /**
   * Why the chosen mapping cannot be played in this scale, if it cannot.
   *
   * Shown rather than left to the player, because a mapping that declines still
   * renders — to consonants over silence — and a listener who pressed a button
   * and heard nothing has no way to tell that from a broken build. The backend
   * refuses the render outright; this is the copy of the same verdict that
   * arrives in time to say so.
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
      // Endpoints have no depth by construction; give them a full bar rather
      // than an empty one, since the tonic and octave are the firmest notes
      // there are.
      weight: d.depth === 0 ? 1 : (deepest > 0 ? d.depth / deepest : 0),
    }));
  });

  ngOnInit(): void {
    this.controls.ensure();
  }

  /**
   * Fetch the scale and point the player at a render, both under the current
   * settings.
   *
   * One button for both because they are one answer: the degrees shown are the
   * degrees that sound, and refreshing either without the other would put a
   * scale on screen that belongs to different audio.
   */
  load(): void {
    this.loading.set(true);
    this.error.set(null);
    const id = this.recordingId();
    const query = this.query();
    this.api.voice(query).subscribe({
      next: (summary) => {
        this.voice.set(summary);
        // Whatever was playing is left alone when the mapping is refused. The
        // render would fail, and an `<audio>` element handed a failing URL shows
        // a broken control and no reason — so replacing audible audio with one
        // would lose both the sound and the explanation.
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
