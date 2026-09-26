import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  type OnInit,
  computed,
  effect,
  inject,
  signal,
  viewChild,
} from "@angular/core";
import { forkJoin } from "rxjs";
import { DecimalPipe } from "@angular/common";
import { MatButtonModule } from "@angular/material/button";
import { MatButtonToggleModule } from "@angular/material/button-toggle";
import { MatCardModule } from "@angular/material/card";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatIconModule } from "@angular/material/icon";
import { MatProgressBarModule } from "@angular/material/progress-bar";
import { MatSelectModule } from "@angular/material/select";

import { ActivatedRoute, Router } from "@angular/router";

import { ControlsStore } from "../../controls-store";
import type { ScoreView } from "../../models";
import { ApiError, RecordingsApi, UNEXPLAINED } from "../../recordings-api";
import { RecordingsStore } from "../../recordings-store";
import { MappingControls } from "../studio/mapping-controls";
import {
  INITIAL_SETTINGS,
  parseSettings,
  settingsQuery,
  type MappingSettings,
} from "../studio/mapping-settings";
import { CompareChart } from "./compare-chart";
import { mostDifferentAt } from "./compare-panels";
import { differences } from "./compare-settings";

/** Which side is audible. */
type Side = "a" | "b";

/**
 * Two settings, heard against each other.
 *
 * Played one after the other, two renders are compared from memory, and a real
 * difference feels like none. So both play at once with one muted, switching at
 * the same instant of the piece; their scores are drawn on top of each other,
 * so a difference too small to hear is still visible; and the chart says where,
 * moving both players there on a click.
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
  /**
   * Sides whose player has been pointed at a new render and cannot play it yet,
   * cleared by the element's own `canplay` or `error`.
   */
  private readonly waiting = signal<ReadonlySet<Side>>(new Set());
  readonly loading = computed(() => this.waiting().size > 0);
  readonly error = signal<string | null>(null);

  /**
   * Why one side cannot be played, if it cannot. Apart from {@link error}
   * because it lasts until the settings move, not until the next attempt.
   */
  readonly unplayable = signal<string | null>(null);

  /** Where the players are, for the chart's playhead. */
  readonly playhead = signal(0);

  /** URLs the two players point at, or `null` before anything was asked for. */
  readonly urlA = signal<string | null>(null);
  readonly urlB = signal<string | null>(null);

  readonly queryA = computed(() => settingsQuery(this.a(), this.controls.knobs()));
  readonly queryB = computed(() => settingsQuery(this.b(), this.controls.knobs()));

  /** What actually differs between the two sides, named. */
  readonly differing = computed(() => differences(this.a(), this.b(), this.controls.knobs()));

  /**
   * True once the *audio* no longer matches the settings, for the take on
   * screen.
   */
  readonly stale = computed(() => {
    const id = this.chosen();
    if (!id || !this.urlA()) return false;
    return (
      this.urlA() !== this.api.renderUrl(id, this.queryA()) ||
      this.urlB() !== this.api.renderUrl(id, this.queryB())
    );
  });

  /**
   * The moment the two renders differ most, in seconds — offered, not jumped to.
   */
  readonly mostDifferent = computed(() => {
    const [a, b] = [this.scoreA(), this.scoreB()];
    if (!a || !b) return null;
    // Across every panel: under `bind` only pitch differs.
    return mostDifferentAt(a, b);
  });

  constructor() {
    this.readUrl();
    this.watchSettings();
    this.writeUrl();
  }

  ngOnInit(): void {
    this.store.refresh();
    this.controls.ensure();
  }

  // ---- the comparison as a link -------------------------------------------
  //
  // A comparison passed on as a description of which controls to move is two
  // people hearing two slightly different things. `a` and `b` each carry a
  // whole settings query, encoded inside this one by `settingsQuery`.

  /**
   * True once the incoming URL has been read. The read waits for the published
   * knobs, and the write waits for the read, or it would overwrite a shared
   * link with defaults.
   */
  private readonly loaded = signal(false);

  private readUrl(): void {
    const route = inject(ActivatedRoute);
    const params = route.snapshot.queryParamMap;

    effect(() => {
      const knobs = this.controls.knobs();
      // One `/api/controls` response carries both lists, so knobs mean mappings
      // are here too.
      const offered = this.controls.mappings();
      if (this.loaded() || knobs.length === 0) return;

      const take = params.get("take");
      if (take) this.recordingId.set(take);
      const a = params.get("a");
      const b = params.get("b");
      if (a !== null) this.a.set(parseSettings(a, knobs, offered));
      // Only with `a` too: one side alone would pair it with a default nobody
      // chose.
      if (a !== null && b !== null) this.b.set(parseSettings(b, knobs, offered, this.b()));
      this.loaded.set(true);
    });
  }

  private writeUrl(): void {
    const router = inject(Router);
    const route = inject(ActivatedRoute);

    effect(() => {
      const knobs = this.controls.knobs();
      if (!this.loaded()) return;
      const queryParams = {
        take: this.chosen(),
        a: settingsQuery(this.a(), knobs),
        b: settingsQuery(this.b(), knobs),
      };
      // `replaceUrl`: the back button should not undo one knob at a time.
      void router.navigate([], { relativeTo: route, queryParams, replaceUrl: true });
    });
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

  /**
   * Keep the charts on whatever the sliders say. A score is about 50 ms, a
   * render seconds, so scores follow the settings and audio waits for the
   * button. Stale responses from a drag are dropped by request token.
   */
  private scoreRequest = 0;

  private watchSettings(): void {
    effect(() => {
      const id = this.chosen();
      const [qa, qb] = [this.queryA(), this.queryB()];
      if (!id) return;

      const token = ++this.scoreRequest;
      forkJoin([this.api.score(id, qa), this.api.score(id, qb)]).subscribe({
        next: ([a, b]) => {
          if (token !== this.scoreRequest) return;
          this.scoreA.set(a);
          this.scoreB.set(b);
          this.error.set(null);
          this.unplayable.set(null);
        },
        error: (err: unknown) => {
          if (token !== this.scoreRequest) return;
          const message = err instanceof ApiError ? err.message : UNEXPLAINED;
          // A refusal is not retried: moving a setting is the way out.
          if (err instanceof ApiError && err.code === "unplayable") {
            this.unplayable.set(message);
            this.error.set(null);
          } else {
            this.error.set(message);
          }
        },
      });
    });
  }

  /** Point the players at renders of the current settings. */
  load(): void {
    const id = this.chosen();
    // Nothing is pointed anywhere while a side is unplayable: a failing
    // `<audio>` shows only that it is broken.
    if (!id || this.unplayable()) return;

    this.error.set(null);
    const urlA = this.api.renderUrl(id, this.queryA());
    const urlB = this.api.renderUrl(id, this.queryB());

    // Only a side whose URL changes is waited on: an unchanged URL loads
    // nothing and never reports it can play.
    const pending = new Set<Side>();
    if (urlA !== this.urlA()) pending.add("a");
    if (urlB !== this.urlB()) pending.add("b");
    this.waiting.set(pending);

    // Both URLs together, so the players never describe different settings.
    this.urlA.set(urlA);
    this.urlB.set(urlB);
    this.playhead.set(0);
  }

  /** A player has enough of its render to start. */
  onReady(side: Side): void {
    this.settle(side);
  }

  /** A player could not load its render. */
  onFailed(side: Side): void {
    this.settle(side);
    this.error.set(`the render for ${side.toUpperCase()} could not be loaded`);
  }

  private settle(side: Side): void {
    const rest = new Set(this.waiting());
    rest.delete(side);
    this.waiting.set(rest);
  }


  // ---- playback -----------------------------------------------------------
  //
  // Two elements in step with one muted, rather than one whose source swaps:
  // swapping reloads and reseeks, and the gap defeats the comparison.

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
    // Started together. `play()` can be refused; say so rather than silently not.
    try {
      await Promise.all(players.map((p) => p.play()));
      this.playing.set(true);
    } catch (err: unknown) {
      this.error.set(err instanceof Error ? err.message : "the browser refused to play");
    }
  }

  /**
   * Switch which side is audible at the same instant, nudging the silent player
   * onto the audible one's clock first — they drift apart over a minute.
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

  /**
   * Move both players to the same moment. A seek before metadata has loaded is
   * silently dropped, so it waits for `loadedmetadata`.
   */
  seekTo(seconds: number): void {
    for (const player of this.both()) {
      if (player.readyState >= HTMLMediaElement.HAVE_METADATA) {
        player.currentTime = seconds;
      } else {
        player.addEventListener("loadedmetadata", () => (player.currentTime = seconds), {
          once: true,
        });
      }
    }
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
