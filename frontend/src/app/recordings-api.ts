import { HttpClient, HttpErrorResponse } from "@angular/common/http";
import { Injectable, inject } from "@angular/core";
import { Observable, throwError } from "rxjs";
import { catchError } from "rxjs/operators";

import type {
  Controls,
  Deleted,
  ErrorBody,
  RecordingDetail,
  RecordingMeta,
  VoiceSummary,
} from "./models";

/**
 * A classified request failure.
 *
 * Classification happens once, at the boundary below, so no callsite ever reads
 * a raw status code. That separation is the point: status 0 (nothing answered)
 * and status 400 (the backend refused this audio) call for completely different
 * words on screen, and they are trivially confusable when each caller squints at
 * `error.status` for itself.
 */
export type ApiFailure =
  | { readonly kind: "offline"; readonly message: string }
  | { readonly kind: "rejected"; readonly code: string; readonly message: string }
  | { readonly kind: "server"; readonly code: string; readonly message: string }
  | { readonly kind: "unknown"; readonly message: string };

/** The error every method in this service rejects with. */
export class ApiError extends Error {
  constructor(readonly failure: ApiFailure) {
    super(failure.message);
    this.name = "ApiError";
  }

  /** Stable code where the backend supplied one, otherwise the failure kind. */
  get code(): string {
    return "code" in this.failure ? this.failure.code : this.failure.kind;
  }
}

/**
 * Turn anything thrown by HttpClient into an {@link ApiFailure}.
 *
 * Exported so it can be tested directly, and so a future caller outside this
 * service classifies the same way rather than inventing a second scheme.
 */
export function classifyApiError(error: unknown): ApiFailure {
  if (!(error instanceof HttpErrorResponse)) {
    return { kind: "unknown", message: error instanceof Error ? error.message : String(error) };
  }

  // Status 0 means the request never got an answer: the backend is not running,
  // or the network is gone. It is emphatically not "the request was refused".
  if (error.status === 0) {
    return { kind: "offline", message: "the backend is not responding" };
  }

  const body = error.error as Partial<ErrorBody> | null;
  const code = body?.code;
  const message = body?.message ?? error.message;

  if (code === undefined) {
    return { kind: "unknown", message };
  }
  return error.status >= 500 ? { kind: "server", code, message } : { kind: "rejected", code, message };
}

/** Rethrow a classified failure, for use in a `catchError`. */
const rethrow = (error: unknown): Observable<never> => throwError(() => new ApiError(classifyApiError(error)));

@Injectable({ providedIn: "root" })
export class RecordingsApi {
  private readonly http = inject(HttpClient);

  list(): Observable<RecordingMeta[]> {
    return this.http.get<RecordingMeta[]>("/api/recordings").pipe(catchError(rethrow));
  }

  get(id: string): Observable<RecordingDetail> {
    return this.http.get<RecordingDetail>(`/api/recordings/${id}`).pipe(catchError(rethrow));
  }

  upload(wav: Blob, label: string): Observable<RecordingDetail> {
    return this.http
      .post<RecordingDetail>("/api/recordings", wav, {
        params: { label },
        headers: { "Content-Type": "audio/wav" },
      })
      .pipe(catchError(rethrow));
  }

  delete(id: string): Observable<Deleted> {
    return this.http.delete<Deleted>(`/api/recordings/${id}`).pipe(catchError(rethrow));
  }

  /**
   * Every control the mapping offers, with the range each one accepts.
   *
   * Fetched rather than written down here: the ranges are facts about the
   * mapping, and a slider offering a value the backend clamps away is a control
   * that appears to do nothing.
   */
  controls(): Observable<Controls> {
    return this.http.get<Controls>("/api/controls").pipe(catchError(rethrow));
  }

  /**
   * The scale, timbre and tonic derived from everything recorded so far.
   *
   * Takes the settings because two of them change the answer: `calibration`
   * picks the take the scale comes from, and `bind` decides how far those
   * degrees are pulled toward equal temperament. Showing a scale that the
   * render will not play would defeat the point of showing it.
   */
  voice(query = ""): Observable<VoiceSummary> {
    return this.http.get<VoiceSummary>(`/api/voice${suffix(query)}`).pipe(catchError(rethrow));
  }

  audioUrl(id: string): string {
    return `/api/recordings/${id}/audio`;
  }

  /**
   * Where this take can be heard as music.
   *
   * A URL rather than a fetched blob: an `<audio>` element streams it, and the
   * backend renders on demand, so nothing here has to hold several megabytes of
   * WAV in memory to play it.
   */
  renderUrl(id: string, query = ""): string {
    return `/api/recordings/${id}/render${suffix(query)}`;
  }
}

/** A query string as a URL suffix: prefixed when there is one, absent when not. */
function suffix(query: string): string {
  return query ? `?${query}` : "";
}
