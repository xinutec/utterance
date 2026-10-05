import { Injectable, computed, inject, signal } from "@angular/core";

import type { RecordingDetail, RecordingMeta, Role, SpeakerCorner } from "./models";
import { ApiError, RecordingsApi, type ApiFailure } from "./recordings-api";
import { Feedback } from "./shared/feedback";

/**
 * The collection of takes, root-provided so it outlives any page — a
 * component-owned list would blank on every navigation.
 */
@Injectable({ providedIn: "root" })
export class RecordingsStore {
  private readonly api = inject(RecordingsApi);
  private readonly feedback = inject(Feedback);

  private readonly stored = signal<readonly RecordingMeta[]>([]);
  /** Takes deleted but still inside their Undo window: hidden, not yet gone. */
  private readonly leaving = signal<ReadonlySet<string>>(new Set());
  readonly recordings = computed(() => this.stored().filter((take) => !this.leaving().has(take.id)));
  readonly selected = signal<RecordingDetail | null>(null);

  /**
   * This speaker's own vowel corners, beside the take list: they describe the
   * speaker, not a take. Empty until the guided vowels are recorded.
   */
  readonly corners = signal<readonly SpeakerCorner[]>([]);
  /** True while a request that the person is waiting on is in flight. */
  readonly busy = signal(false);
  readonly error = signal<string | null>(null);

  refresh(): void {
    this.api.list().subscribe({
      next: (list) => {
        this.stored.set(list);
      },
      error: (err: unknown) => {
        this.fail(err);
      },
    });
    this.refreshCorners();
  }

  /**
   * Re-read the speaker's corners — part of `refresh`, which every change to
   * the calibration set ends in. A failure keeps the old corners quietly: the
   * chart falls back to generic positions and says so.
   */
  private refreshCorners(): void {
    this.api.speakerCorners().subscribe({
      next: (speaker) => {
        this.corners.set(speaker.corners);
      },
      // dev-lint: allow-ignored-error keeps the old corners; the chart falls back to generic positions and says so
      error: () => {},
    });
  }

  /** Load a take's voiceprint, for its screen and for the calibration check. */
  open(id: string): void {
    this.busy.set(true);
    this.error.set(null);
    this.api.get(id).subscribe({
      next: (detail) => {
        this.selected.set(detail);
        this.busy.set(false);
      },
      error: (err: unknown) => {
        this.fail(err);
      },
    });
  }

  /** Store a take and open it; `stored` hears its id, to navigate there. */
  upload(wav: Blob, label: string, role: Role = "material", stored?: (id: string) => void): void {
    this.busy.set(true);
    this.error.set(null);
    this.api.upload(wav, label, role).subscribe({
      next: (detail) => {
        this.selected.set(detail);
        this.busy.set(false);
        stored?.(detail.meta.id);
        this.refresh();
      },
      error: (err: unknown) => {
        this.fail(err);
      },
    });
  }

  /**
   * Say whether a stored take defines the voice. Refreshes the whole list,
   * since the role changes the scale, the vowel space and the tonic.
   */
  setRole(meta: RecordingMeta, role: Role): void {
    this.busy.set(true);
    this.error.set(null);
    this.api.setRole(meta.id, role).subscribe({
      next: () => {
        this.busy.set(false);
        if (this.selected()?.meta.id === meta.id) this.open(meta.id);
        this.refresh();
      },
      error: (err: unknown) => {
        this.fail(err);
      },
    });
  }

  /**
   * Hide a take and delete it only once Undo has not been taken: the audio is
   * the one thing here that cannot be re-derived.
   */
  remove(meta: RecordingMeta): void {
    this.leaving.update((ids) => new Set(ids).add(meta.id));
    if (this.selected()?.meta.id === meta.id) this.selected.set(null);
    this.feedback.undo(
      `Deleted ${meta.label}`,
      () => {
        this.release(meta.id);
      },
      () => {
        this.api.delete(meta.id).subscribe({
          next: () => {
            // Dropped before it is released, so it does not flash back
            // while the list reloads.
            this.stored.update((list) => list.filter((take) => take.id !== meta.id));
            this.release(meta.id);
            this.refresh();
          },
          error: (err: unknown) => {
            this.release(meta.id);
            this.fail(err);
          },
        });
      },
    );
  }

  /** End `id`'s Undo window, whichever way it ended. */
  private release(id: string): void {
    this.leaving.update((ids) => {
      const rest = new Set(ids);
      rest.delete(id);
      return rest;
    });
  }

  audioUrl(id: string): string {
    return this.api.audioUrl(id);
  }

  clearError(): void {
    this.error.set(null);
  }

  private fail(err: unknown): void {
    this.busy.set(false);
    this.error.set(err instanceof ApiError ? explain(err.failure) : String(err));
  }
}

/**
 * Wording for each failure class: what happened and what to do. A rejected
 * recording uses the backend's message, which says how short it was.
 */
function explain(failure: ApiFailure): string {
  switch (failure.kind) {
    case "offline":
      return "the backend is not responding — is `scripts/dev.sh` still running?";
    case "rejected":
      return failure.message;
    case "server":
      return `the backend failed to handle that (${failure.code}) — check its log`;
    case "unknown":
      return failure.message;
  }
}
