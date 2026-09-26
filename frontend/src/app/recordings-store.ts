import { Injectable, inject, signal } from "@angular/core";

import type { RecordingDetail, RecordingMeta, Role, SpeakerCorner } from "./models";
import { ApiError, RecordingsApi, type ApiFailure } from "./recordings-api";

/**
 * The collection of takes, root-provided so it outlives any page — a
 * component-owned list would blank on every navigation.
 */
@Injectable({ providedIn: "root" })
export class RecordingsStore {
  private readonly api = inject(RecordingsApi);

  readonly recordings = signal<readonly RecordingMeta[]>([]);
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
        this.recordings.set(list);
        // Open the newest take: usually the one just recorded.
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
   * Re-read the speaker's corners — part of `refresh`, which every change to
   * the calibration set ends in. A failure keeps the old corners quietly: the
   * chart falls back to generic positions and says so.
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
   * Say whether a stored take defines the voice. Refreshes the whole list,
   * since the role changes the scale, the vowel space and the tonic.
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
