import { Injectable, inject, signal } from "@angular/core";

import type { RecordingDetail, RecordingMeta, Role, SpeakerCorner } from "./models";
import { ApiError, RecordingsApi, type ApiFailure } from "./recordings-api";

/**
 * The collection of takes, held for the lifetime of the app.
 *
 * Root-provided rather than owned by the studio component: a component-local
 * list is emptied and re-fetched every time the component is destroyed and
 * recreated, so the page blanks on every navigation. The list is application
 * state, not view state.
 */
@Injectable({ providedIn: "root" })
export class RecordingsStore {
  private readonly api = inject(RecordingsApi);

  readonly recordings = signal<readonly RecordingMeta[]>([]);
  readonly selected = signal<RecordingDetail | null>(null);

  /**
   * This speaker's own vowel corners, from the guided vowels.
   *
   * Application state beside the take list rather than something a chart fetches
   * for itself: they describe the speaker, so they are the same for every take
   * on screen and they change only when a calibration take does. Empty until the
   * guided vowels have been recorded.
   */
  readonly corners = signal<readonly SpeakerCorner[]>([]);
  /** True while a request that the person is waiting on is in flight. */
  readonly busy = signal(false);
  readonly error = signal<string | null>(null);

  refresh(): void {
    this.api.list().subscribe({
      next: (list) => {
        this.recordings.set(list);
        // Open the newest take automatically: the common case is having just
        // recorded something and wanting to see it.
        const [newest] = list;
        if (!this.selected() && newest) this.select(newest);
      },
      error: (err: unknown) => {
        this.fail(err);
      },
    });
    this.refreshCorners();
  }

  /**
   * Re-read the speaker's corners.
   *
   * Folded into `refresh` because every route that changes which takes define
   * the speaker — recording one, deleting one, changing a role — already ends
   * there. A corner list that kept describing a deleted take would be a claim
   * about a mouth, sourced from nothing.
   *
   * A failure here leaves the corners as they were and does not raise the
   * page's error: this is the chart's reference grid, and losing it is not worth
   * covering the screen a take was just recorded onto. The chart falls back to
   * generic positions and says so.
   */
  private refreshCorners(): void {
    this.api.speakerCorners().subscribe({
      next: (speaker) => {
        this.corners.set(speaker.corners);
      },
      error: () => {},
    });
  }

  select(meta: RecordingMeta): void {
    this.busy.set(true);
    this.api.get(meta.id).subscribe({
      next: (detail) => {
        this.selected.set(detail);
        this.busy.set(false);
      },
      error: (err: unknown) => {
        this.fail(err);
      },
    });
  }

  upload(wav: Blob, label: string, role: Role = "material"): void {
    this.busy.set(true);
    this.error.set(null);
    this.api.upload(wav, label, role).subscribe({
      next: (detail) => {
        this.selected.set(detail);
        this.busy.set(false);
        this.refresh();
      },
      error: (err: unknown) => {
        this.fail(err);
      },
    });
  }

  /**
   * Say whether a stored take defines the voice or is only material.
   *
   * Refreshes the list rather than patching the row in place: the role decides
   * which takes the speaker is derived from, so changing it changes the scale,
   * the vowel space and the tonic — and a list that quietly disagreed with the
   * voice on screen would be worse than a reload.
   */
  setRole(meta: RecordingMeta, role: Role): void {
    this.busy.set(true);
    this.error.set(null);
    this.api.setRole(meta.id, role).subscribe({
      next: () => {
        this.busy.set(false);
        if (this.selected()?.meta.id === meta.id) this.select({ ...meta, role });
        this.refresh();
      },
      error: (err: unknown) => {
        this.fail(err);
      },
    });
  }

  remove(meta: RecordingMeta): void {
    this.api.delete(meta.id).subscribe({
      next: () => {
        if (this.selected()?.meta.id === meta.id) this.selected.set(null);
        this.refresh();
      },
      error: (err: unknown) => {
        this.fail(err);
      },
    });
  }

  audioUrl(id: string): string {
    return this.api.audioUrl(id);
  }

  setError(message: string): void {
    this.error.set(message);
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
 * Wording for each failure class.
 *
 * Every branch says what happened and, where there is one, what to do about it.
 * The backend's message is used verbatim for a rejected recording because it is
 * specific — how short the take was, how far off the minimum.
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
