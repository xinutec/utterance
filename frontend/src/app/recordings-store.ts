import { Injectable, inject, signal } from "@angular/core";

import type { RecordingDetail, RecordingMeta } from "./models";
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
  /** True while a request that the person is waiting on is in flight. */
  readonly busy = signal(false);
  readonly error = signal<string | null>(null);

  refresh(): void {
    this.api.list().subscribe({
      next: (list) => {
        this.recordings.set(list);
        // Open the newest take automatically: the common case is having just
        // recorded something and wanting to see it.
        if (!this.selected() && list.length > 0) this.select(list[0]);
      },
      error: (err: unknown) => {
        this.fail(err);
      },
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

  upload(wav: Blob, label: string): void {
    this.busy.set(true);
    this.error.set(null);
    this.api.upload(wav, label).subscribe({
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
