import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  type OnInit,
  computed,
  inject,
  signal,
  viewChild,
} from "@angular/core";
import { DecimalPipe } from "@angular/common";
import { MatButtonModule } from "@angular/material/button";
import { MatButtonToggleModule } from "@angular/material/button-toggle";
import { MatCardModule } from "@angular/material/card";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatIconModule } from "@angular/material/icon";
import { MatProgressBarModule } from "@angular/material/progress-bar";
import { MatSelectModule } from "@angular/material/select";

import { ControlsStore } from "../../controls-store";
import type { ScoreView } from "../../models";
import { ApiError, RecordingsApi } from "../../recordings-api";
import { RecordingsStore } from "../../recordings-store";
import { MappingControls } from "../studio/mapping-controls";
import { INITIAL_SETTINGS, settingsQuery, type MappingSettings } from "../studio/mapping-settings";
import { CompareChart } from "./compare-chart";
import { differences, divergence, loudestDifference } from "./compare-settings";

/** Which side is audible. */
type Side = "a" | "b";

/**
 * Two settings, heard against each other.
 *
 * **The problem this solves.** Two renders played one after the other are
 * separated by however long the first one lasted, and by then the ear has
 * nothing left to compare against — a real difference and no difference feel
 * identical. Every question this project has left open is of that shape: is the
 * speaker's tuning better than equal temperament, does a knob earn its place.
 *
 * Three things make the comparison answerable, and all three matter:
 *
 * - **Both renders play at once**, with one muted. Switching is instant and at
 *   the same instant of the piece, so what is being compared is two versions of
 *   one moment rather than two memories.
 * - **The scores are drawn on top of each other**, so a difference too small to
 *   hear is still visible — which is the difference between "this knob does
 *   nothing" and "this knob does something I cannot hear yet".
 * - **The chart says where.** Clicking moves both players there. Nobody has to
 *   hunt through 46 seconds for the four that differ.
 */
@Component({
  selector: "app-compare",
  templateUrl: "./compare.html",
  styleUrl: "./compare.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    DecimalPipe,
    MatButtonModule,
    MatButtonToggleModule,
    MatCardModule,
    MatFormFieldModule,
    MatIconModule,
    MatProgressBarModule,
    MatSelectModule,
    CompareChart,
    MappingControls,
  ],
})
export class Compare implements OnInit {
  private readonly api = inject(RecordingsApi);
  readonly store = inject(RecordingsStore);
  readonly controls = inject(ControlsStore);

  readonly recordingId = signal<string | null>(null);
  readonly a = signal<MappingSettings>(INITIAL_SETTINGS);
  readonly b = signal<MappingSettings>({ ...INITIAL_SETTINGS, knobs: { bind: 0 } });

  /** Which side is audible right now. */
  readonly side = signal<Side>("a");
  /** Whether the settings panels are open — they are large and rarely needed. */
  readonly editing = signal(false);

  readonly scoreA = signal<ScoreView | null>(null);
  readonly scoreB = signal<ScoreView | null>(null);
  readonly loading = signal(false);
  readonly error = signal<string | null>(null);

  /** Where the players are, for the chart's playhead. */
  readonly playhead = signal(0);

  /** URLs the two players point at, or `null` before anything was asked for. */
  readonly urlA = signal<string | null>(null);
  readonly urlB = signal<string | null>(null);

  readonly queryA = computed(() => settingsQuery(this.a(), this.controls.knobs()));
  readonly queryB = computed(() => settingsQuery(this.b(), this.controls.knobs()));

  /** What actually differs between the two sides, named. */
  readonly differing = computed(() => differences(this.a(), this.b(), this.controls.knobs()));

  /** True once the loaded renders no longer match the current settings. */
  readonly stale = computed(() => {
    const id = this.recordingId();
    if (!id || !this.urlA()) return false;
    return (
      this.urlA() !== this.api.renderUrl(id, this.queryA()) ||
      this.urlB() !== this.api.renderUrl(id, this.queryB())
    );
  });

  /**
   * The moment the two renders differ most, in seconds.
   *
   * Offered rather than jumped to: it is a suggestion about where to listen, and
   * moving someone's playhead without being asked is not.
   */
  readonly mostDifferent = computed(() => {
    const [a, b] = [this.scoreA(), this.scoreB()];
    if (!a || !b || a.colour.length === 0) return null;
    // Across the streams a listener is most likely to notice, rather than one:
    // a change that shows in none of these is not one anyone will hear.
    const combined = [a.level, a.colour, a.breath]
      .map((stream, i) => divergence(stream, [b.level, b.colour, b.breath][i]))
      .reduce((acc, curve) => acc.map((v, j) => Math.max(v, curve[j] ?? 0)));
    return loudestDifference(combined, a.stepS);
  });

  ngOnInit(): void {
    this.store.refresh();
    this.controls.ensure();
  }

  /** Default to the longest take, which is the one with the most to compare. */
  readonly chosen = computed(() => {
    const explicit = this.recordingId();
    if (explicit) return explicit;
    const takes = this.store.recordings();
    if (takes.length === 0) return null;
    return takes.reduce((best, t) => (t.durationS > best.durationS ? t : best)).id;
  });

  /** A scale as something to read: whole cents, comma separated. */
  degreeList(degrees: readonly number[]): string {
    return degrees.map((c) => Math.round(c)).join(", ");
  }

  choose(id: string): void {
    this.recordingId.set(id);
  }

  /** Copy one side's settings onto the other, as a base for a small change. */
  copyAcross(): void {
    this.b.set({ ...this.a() });
  }

  swap(): void {
    const [a, b] = [this.a(), this.b()];
    this.a.set(b);
    this.b.set(a);
  }

  load(): void {
    const id = this.chosen();
    if (!id) return;

    this.loading.set(true);
    this.error.set(null);
    const [qa, qb] = [this.queryA(), this.queryB()];

    // Both scores before either player moves, so the chart and the audio always
    // describe the same pair. Loading them one at a time would leave a window
    // where the chart compares one new render against one old one.
    this.api.score(id, qa).subscribe({
      next: (a) => {
        this.api.score(id, qb).subscribe({
          next: (b) => {
            this.scoreA.set(a);
            this.scoreB.set(b);
            this.urlA.set(this.api.renderUrl(id, qa));
            this.urlB.set(this.api.renderUrl(id, qb));
            this.playhead.set(0);
            this.loading.set(false);
          },
          error: (err: unknown) => this.fail(err),
        });
      },
      error: (err: unknown) => this.fail(err),
    });
  }

  private fail(err: unknown): void {
    this.loading.set(false);
    this.error.set(err instanceof ApiError ? err.message : String(err));
  }

  // ---- playback -----------------------------------------------------------
  //
  // Two elements playing in step with one muted, rather than one element whose
  // source is swapped. Swapping a source reloads and reseeks, which takes long
  // enough to hear as a gap — and a gap is precisely the thing that makes two
  // renders impossible to compare.

  private readonly playerA = viewChild<ElementRef<HTMLAudioElement>>("playerA");
  private readonly playerB = viewChild<ElementRef<HTMLAudioElement>>("playerB");

  readonly playing = signal(false);

  private both(): HTMLAudioElement[] {
    return [this.playerA()?.nativeElement, this.playerB()?.nativeElement].filter(
      (el): el is HTMLAudioElement => !!el,
    );
  }

  private audible(): HTMLAudioElement | undefined {
    const el = this.side() === "a" ? this.playerA() : this.playerB();
    return el?.nativeElement;
  }

  async toggle(): Promise<void> {
    const players = this.both();
    if (players.length < 2) return;

    if (this.playing()) {
      players.forEach((p) => p.pause());
      this.playing.set(false);
      return;
    }

    this.applySide();
    // Started together from the same position. Play returns a promise that
    // rejects if the browser refuses; nothing here can be done about that, but
    // reporting it beats a button that silently does nothing.
    try {
      await Promise.all(players.map((p) => p.play()));
      this.playing.set(true);
    } catch (err: unknown) {
      this.error.set(err instanceof Error ? err.message : "the browser refused to play");
    }
  }

  /**
   * Switch which side is audible, at the same instant of the piece.
   *
   * The silent player is nudged onto the audible one's clock first. Two elements
   * decoding independently drift by a few milliseconds over a minute, and a
   * switch that also jumps in time is a switch that tells you nothing about the
   * difference between the renders.
   */
  chooseSide(side: Side): void {
    const from = this.audible();
    this.side.set(side);
    const to = this.audible();
    if (from && to && to !== from && Math.abs(to.currentTime - from.currentTime) > 0.02) {
      to.currentTime = from.currentTime;
    }
    this.applySide();
  }

  seekTo(seconds: number): void {
    this.both().forEach((p) => {
      p.currentTime = seconds;
    });
    this.playhead.set(seconds);
  }

  onTimeUpdate(): void {
    const el = this.audible();
    if (el) this.playhead.set(el.currentTime);
  }

  onEnded(): void {
    this.playing.set(false);
  }

  /** One player at full volume, the other silent but still running. */
  private applySide(): void {
    const [a, b] = [this.playerA()?.nativeElement, this.playerB()?.nativeElement];
    if (a) a.muted = this.side() !== "a";
    if (b) b.muted = this.side() !== "b";
  }
}
