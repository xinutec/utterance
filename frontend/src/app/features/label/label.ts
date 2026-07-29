import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  effect,
  type OnDestroy,
  type OnInit,
  computed,
  inject,
  signal,
  viewChild,
} from "@angular/core";
import { ActivatedRoute, Router } from "@angular/router";
import { DecimalPipe } from "@angular/common";
import { MatButtonModule } from "@angular/material/button";
import { MatButtonToggleModule } from "@angular/material/button-toggle";
import { MatCardModule } from "@angular/material/card";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatIconModule } from "@angular/material/icon";
import { MatSelectModule } from "@angular/material/select";

import { Help } from "../../help";
import type { Syllable, Voiceprint } from "../../models";
import { LabelChart } from "./label-chart";
import { ApiError, RecordingsApi } from "../../recordings-api";
import { RecordingsStore } from "../../recordings-store";

/** How long after the last tap the marks are written, in milliseconds. */
const AUTOSAVE_MS = 1500;

/**
 * Marking where syllables begin, by ear.
 *
 * **Why the app collects this rather than an external editor.** Two gaps in the
 * analysis layer are blocked on ground truth nobody has produced — onsets mean
 * *the spectrum changed* rather than *a syllable began*, and there is no way to
 * score a formant tracker against a real take. Both need somebody's ear, and a
 * label track living in another program's file drifts from the recording, does
 * not survive a change of machine, and cannot be shared between two people who
 * are already signed in here.
 *
 * **Tapping is deliberately imprecise, and that is the plan.** Human reaction
 * lag is 150–250 ms and varies, so a tapped mark is systematically late. This
 * page collects the taps; placing them exactly is the next stage, against the
 * flux curve — which is the measurement the labels will judge, so putting truth
 * beside it is the point.
 *
 * What this will never do is pre-place marks from the onset detector for
 * somebody to correct. That would be grading its homework against its own
 * answers.
 */
@Component({
  selector: "app-label",
  templateUrl: "./label.html",
  styleUrl: "./label.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    DecimalPipe,
    Help,
    LabelChart,
    MatButtonModule,
    MatButtonToggleModule,
    MatCardModule,
    MatFormFieldModule,
    MatIconModule,
    MatSelectModule,
  ],
  host: {
    // Tapping wants a key under a finger that is already resting. Space is the
    // obvious one and is taken: the browser gives it to the player, and losing
    // the transport mid-take is worse than an unfamiliar key.
    "(document:keydown.m)": "mark()",
  },
})
export class Label implements OnInit, OnDestroy {
  private readonly api = inject(RecordingsApi);
  readonly store = inject(RecordingsStore);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);

  readonly recordingId = signal<string | null>(null);
  readonly syllables = signal<readonly Syllable[]>([]);
  readonly saved = signal(true);
  readonly error = signal<string | null>(null);

  /** Where the player is, so a mark lands where the ear was. */
  readonly playhead = signal(0);

  /**
   * How fast the take plays back.
   *
   * A consonant onset at a quarter speed is four times as wide in time, which
   * is the difference between hearing *where* a syllable starts and hearing
   * *that* it started. Pitch goes with it — the browser does not preserve it at
   * these rates and should not, since what is being listened for is an edge
   * rather than a note.
   */
  readonly speed = signal(1);

  /** The rates offered. A quarter is where a plosive stops being an instant. */
  readonly speeds = [0.25, 0.5, 1] as const;

  setSpeed(rate: number): void {
    this.speed.set(rate);
    const player = this.player()?.nativeElement;
    if (player) player.playbackRate = rate;
  }

  /**
   * The take's own measurements, for the level curve marks are placed against.
   *
   * Fetched here rather than derived: a mark is only as good as what it can be
   * seen against, and at the width that makes a syllable placeable there is no
   * fitting a take on a screen — see `LabelChart`.
   */
  readonly voiceprint = signal<Voiceprint | null>(null);

  private readonly player = viewChild<ElementRef<HTMLAudioElement>>("player");
  private pending: ReturnType<typeof setTimeout> | null = null;

  /** Default to the longest take: the one with the most to mark. */
  readonly chosen = computed(() => {
    const explicit = this.recordingId();
    if (explicit) return explicit;
    const takes = this.store.recordings();
    if (takes.length === 0) return null;
    return takes.reduce((best, t) => (t.durationS > best.durationS ? t : best)).id;
  });

  readonly audioUrl = computed(() => {
    const id = this.chosen();
    return id ? this.api.audioUrl(id) : null;
  });

  /** Fewest marks before a rate says anything about a session. */
  private static readonly ENOUGH = 8;

  /**
   * How fast the marked syllables come, from the *median* gap between them.
   *
   * **Median, not marks-per-elapsed-second.** A take is mostly silence — the
   * speech one is 28% voiced — so dividing by the span counts every pause as
   * slow speaking and reports about 1.2/s for a flawless session. The middle gap
   * ignores the pauses and describes the speaking, which is what the number is
   * for: three to eight a second is ordinary, and anything outside it means taps
   * were missed or doubled.
   *
   * Absent until there are enough marks to have a middle at all. Two marks are
   * one interval, and calling that a rate told somebody their careful work was
   * wrong.
   */
  readonly rate = computed(() => {
    const marks = this.syllables();
    if (marks.length < Label.ENOUGH) return null;
    const gaps = marks.slice(1).map((mark, i) => mark.atS - marks[i].atS);
    gaps.sort((a, b) => a - b);
    const median = gaps[Math.floor(gaps.length / 2)];
    return median > 0 ? 1 / median : null;
  });

  constructor() {
    // **Loading follows the choice rather than the page.** `chosen()` falls back
    // to the longest take, which does not exist until the list arrives — so a
    // load fired once on init runs while there is nothing to load, returns, and
    // is never tried again. The symptom is a page that looks like a take with no
    // marks, which is indistinguishable from a take nobody has marked.
    effect(() => {
      const id = this.chosen();
      if (id) this.load(id);
    });
  }

  ngOnInit(): void {
    this.store.refresh();
    const take = this.route.snapshot.queryParamMap.get("take");
    if (take) this.recordingId.set(take);
  }

  ngOnDestroy(): void {
    // A session's marks are somebody's ear spent once; leaving the page must not
    // be how they are lost.
    if (this.pending) {
      clearTimeout(this.pending);
      this.write();
    }
  }

  choose(id: string): void {
    this.flush();
    this.recordingId.set(id);
    void this.router.navigate([], {
      relativeTo: this.route,
      queryParams: { take: id },
      replaceUrl: true,
    });
  }

  private load(id: string): void {
    // dev-lint: allow-component-list re-fetching on return is the correct
    // behaviour here rather than a loss. The rule guards against a list that
    // blanks and re-requests, taking unsaved state with it; these marks are
    // written to the store 1.5s after every change and flushed again on
    // destroy, so the server always holds the truth and reading it back is how
    // this page learns what the other listener marked. Holding them in a
    // root-provided store would instead keep one take's marks alive while
    // showing another's.
    this.api.get(id).subscribe({
      next: (detail) => this.voiceprint.set(detail.voiceprint),
      error: (err: unknown) => this.fail(err),
    });
    this.api.labels(id).subscribe({
      next: (labels) => {
        this.syllables.set(labels.syllables);
        this.saved.set(true);
      },
      error: (err: unknown) => this.fail(err),
    });
  }

  /** Mark a syllable at the playhead. */
  mark(): void {
    const player = this.player()?.nativeElement;
    if (!player) return;
    this.syllables.update((marks) =>
      [...marks, { atS: player.currentTime }].sort((a, b) => a.atS - b.atS),
    );
    this.touched();
  }

  /** A mark was placed, moved or removed on the chart. */
  touchedFromChart(): void {
    this.touched();
  }

  /**
   * True while a mark is being dragged.
   *
   * Saving waits for the hand to let go. A write mid-drag answers with the
   * stored list *sorted*, and the reordering arriving under a gesture in
   * progress is how a drag started moving the wrong mark.
   */
  private editing = false;

  onEditing(editing: boolean): void {
    this.editing = editing;
    if (!editing) this.touched();
  }

  remove(at: number): void {
    this.syllables.update((marks) => marks.filter((m) => m.atS !== at));
    this.touched();
  }

  clear(): void {
    this.syllables.set([]);
    this.touched();
  }

  /** Move the player to a mark, to hear whether it sits where it should. */
  seekTo(seconds: number): void {
    const player = this.player()?.nativeElement;
    if (!player) return;
    if (player.readyState >= HTMLMediaElement.HAVE_METADATA) {
      player.currentTime = seconds;
    } else {
      player.addEventListener("loadedmetadata", () => (player.currentTime = seconds), {
        once: true,
      });
    }
  }

  onTimeUpdate(): void {
    const player = this.player()?.nativeElement;
    if (player) this.playhead.set(player.currentTime);
  }

  /**
   * Written a moment after the last change rather than on every tap.
   *
   * Somebody marking in time produces several taps a second, and a request each
   * would put the store behind the hand. Waiting for the tapping to stop keeps
   * one write per phrase — and the timer is cleared on leaving the page, so the
   * delay never costs a session.
   */
  private touched(): void {
    this.saved.set(false);
    if (this.pending) clearTimeout(this.pending);
    if (this.editing) return;
    this.pending = setTimeout(() => this.write(), AUTOSAVE_MS);
  }

  /** Write now, if anything is waiting. */
  flush(): void {
    if (!this.pending) return;
    clearTimeout(this.pending);
    this.write();
  }

  private write(): void {
    this.pending = null;
    const id = this.chosen();
    if (!id) return;
    this.api.putLabels(id, { syllables: [...this.syllables()] }).subscribe({
      // The stored set, not the sent one: the backend orders the marks and
      // merges any two closer than a syllable can be, so the screen would
      // otherwise disagree with the store the first time somebody double-tapped.
      next: (stored) => {
        this.syllables.set(stored.syllables);
        this.saved.set(true);
        this.error.set(null);
      },
      error: (err: unknown) => this.fail(err),
    });
  }

  private fail(err: unknown): void {
    this.error.set(err instanceof ApiError ? err.message : String(err));
  }
}
