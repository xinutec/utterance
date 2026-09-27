import {
  ChangeDetectionStrategy,
  Component,
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
import { MatCardModule } from "@angular/material/card";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatIconModule } from "@angular/material/icon";
import { MatSelectModule } from "@angular/material/select";

import { ActivatedRoute, Router } from "@angular/router";
import { scaffoldTitle } from "@xinutec/ui-scaffold";

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
import { AbPlayer, type Side } from "./ab-player";
import { CompareChart } from "./compare-chart";
import { mostDifferentAt } from "./compare-panels";
import { differences } from "./compare-settings";

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
    MatCardModule,
    MatFormFieldModule,
    MatIconModule,
    MatSelectModule,
    AbPlayer,
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

  /** Which side is audible right now; the player owns switching it. */
  readonly side = signal<Side>("a");
  /** Whether the settings panels are open — they are large and rarely needed. */
  readonly editing = signal(false);

  readonly scoreA = signal<ScoreView | null>(null);
  readonly scoreB = signal<ScoreView | null>(null);
  /** Whether the player has a new render it cannot play yet. */
  readonly loading = signal(false);
  readonly error = signal<string | null>(null);

  /**
   * Why one side cannot be played, if it cannot. Apart from {@link error}
   * because it lasts until the settings move, not until the next attempt.
   */
  readonly unplayable = signal<string | null>(null);

  /** Where the players are, for the chart's playhead. */
  readonly playhead = signal(0);

  /** The player, which the chart seeks on a click. */
  protected readonly player = viewChild(AbPlayer);

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
    scaffoldTitle(() => "Compare");
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
    // Both URLs together, so the players never describe different settings.
    this.urlA.set(this.api.renderUrl(id, this.queryA()));
    this.urlB.set(this.api.renderUrl(id, this.queryB()));
    this.playhead.set(0);
  }
}
