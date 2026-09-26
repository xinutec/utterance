import { HttpClient, HttpErrorResponse } from "@angular/common/http";
import { Injectable, inject } from "@angular/core";
import { Observable, throwError } from "rxjs";
import { catchError } from "rxjs/operators";

import type {
  Controls,
  Deleted,
  ErrorCode,
  RecordingDetail,
  RecordingMeta,
  Role,
  ScoreView,
  SpeakerCorners,
  TelemetryEvent,
  VoiceSummary,
} from "./models";

/**
 * A classified request failure. Classified once, here, so no caller reads a raw
 * status: 0 (nothing answered) and 400 (the audio was refused) need different
 * words on screen.
 */
export type ApiFailure =
  | { readonly kind: "offline"; readonly message: string }
  | { readonly kind: "rejected"; readonly code: ErrorCode; readonly message: string }
  | { readonly kind: "server"; readonly code: ErrorCode; readonly message: string }
  | { readonly kind: "unknown"; readonly message: string };

/**
 * What to say when nothing about a failure could be recognised — never
 * `String(err)`, which reads "[object Object]".
 */
export const UNEXPLAINED = "the server did not say what went wrong";

/** The error every method in this service rejects with. */
export class ApiError extends Error {
  constructor(readonly failure: ApiFailure) {
    super(failure.message);
    this.name = "ApiError";
  }

  /**
   * Stable code where the backend supplied one, otherwise the failure kind.
   * Typed, not `string`, so a comparison against a code that does not exist
   * fails to compile.
   */
  get code(): ErrorCode | "offline" | "unknown" {
    return "code" in this.failure ? this.failure.code : this.failure.kind;
  }
}

/** Whether a value is an object whose fields can be read by name. */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/**
 * The named field of an unknown value, only if it really is a string.
 * `error.error` is whatever came back — an ingress 502 sends HTML — so it is
 * checked, not asserted to be an `ErrorBody`.
 */
function stringField(value: unknown, key: string): string | null {
  if (!isRecord(value)) return null;
  const field = value[key];
  return typeof field === "string" && field !== "" ? field : null;
}

/**
 * Every code the backend can send, as values. `Record<ErrorCode, true>` makes a
 * code added in `src/error.rs` fail the build here until it is listed.
 */
const CODES: Readonly<Record<ErrorCode, true>> = {
  audio_undecodable: true,
  audio_empty: true,
  audio_too_short: true,
  not_found: true,
  record_corrupt: true,
  storage_io: true,
  bad_request: true,
  unplayable: true,
  no_calibration: true,
  not_authenticated: true,
  not_permitted: true,
  bad_login_state: true,
  no_authorization_code: true,
  sign_in_failed: true,
};

/**
 * The `code` of an error body, only if it is one this backend defines; anything
 * else is classified `unknown` and explained by its message.
 */
function errorCode(value: unknown): ErrorCode | null {
  const field = stringField(value, "code");
  return field !== null && isCode(field) ? field : null;
}

function isCode(value: string): value is ErrorCode {
  return Object.hasOwn(CODES, value);
}

/** Turn anything thrown by HttpClient into an {@link ApiFailure}. */
export function classifyApiError(error: unknown): ApiFailure {
  if (!(error instanceof HttpErrorResponse)) {
    // Not `String(error)`, which reads "[object Object]" for a plain object.
    const message = error instanceof Error ? error.message : "something went wrong";
    return { kind: "unknown", message };
  }

  // Status 0: no answer at all — backend down or network gone, not a refusal.
  if (error.status === 0) {
    return { kind: "offline", message: "the backend is not responding" };
  }

  const code = errorCode(error.error);
  const message = stringField(error.error, "message") ?? error.message;

  if (code === null) {
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

  /**
   * Store a take. `role` defaults to material: only the guided calibration
   * flow sends `calibration`, so an upload shapes the speaker only on purpose.
   */
  upload(wav: Blob, label: string, role: Role = "material"): Observable<RecordingDetail> {
    return this.http
      .post<RecordingDetail>("/api/recordings", wav, {
        params: { label, role },
        headers: { "Content-Type": "audio/wav" },
      })
      .pipe(catchError(rethrow));
  }

  /**
   * Say what an already-stored take is for — a file upload, or a take from
   * before roles existed, may need to become a calibration one.
   */
  setRole(id: string, role: Role): Observable<RecordingMeta> {
    return this.http
      .put<RecordingMeta>(`/api/recordings/${id}/role`, { role })
      .pipe(catchError(rethrow));
  }

  /**
   * Send a batch of client events to be logged. The caller ignores failures: a
   * trace must not interfere with the app it observes.
   */
  sendTelemetry(events: readonly TelemetryEvent[]): Observable<void> {
    return this.http.post<void>("/api/telemetry", events);
  }

  delete(id: string): Observable<Deleted> {
    return this.http.delete<Deleted>(`/api/recordings/${id}`).pipe(catchError(rethrow));
  }

  /** Every control the mapping offers, with the range each one accepts. */
  controls(): Observable<Controls> {
    return this.http.get<Controls>("/api/controls").pipe(catchError(rethrow));
  }

  /**
   * The scale, timbre and tonic derived from the speaker's calibration takes,
   * under the given settings — `calibration` and `bind` change the answer.
   */
  voice(query = ""): Observable<VoiceSummary> {
    return this.http.get<VoiceSummary>(`/api/voice${suffix(query)}`).pipe(catchError(rethrow));
  }

  /**
   * This speaker's own vowel corners. Separate from `voice()`: it needs no
   * scale, so it answers even when the takes are too short for one.
   */
  speakerCorners(): Observable<SpeakerCorners> {
    return this.http.get<SpeakerCorners>("/api/speaker/corners").pipe(catchError(rethrow));
  }

  /** What the render with the same parameters is made of. */
  score(id: string, query = ""): Observable<ScoreView> {
    return this.http
      .get<ScoreView>(`/api/recordings/${id}/score${suffix(query)}`)
      .pipe(catchError(rethrow));
  }

  audioUrl(id: string): string {
    return `/api/recordings/${id}/audio`;
  }

  /**
   * Where this take can be heard as music: a URL, so the `<audio>` element
   * streams the render rather than this holding megabytes of WAV.
   */
  renderUrl(id: string, query = ""): string {
    return `/api/recordings/${id}/render${suffix(query)}`;
  }
}

/** A query string as a URL suffix: prefixed when there is one, absent when not. */
function suffix(query: string): string {
  return query ? `?${query}` : "";
}
