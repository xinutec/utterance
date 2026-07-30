import { HttpClient, HttpErrorResponse } from "@angular/common/http";
import { Injectable, inject } from "@angular/core";
import { Observable, throwError } from "rxjs";
import { catchError } from "rxjs/operators";

import type {
  Controls,
  Deleted,
  RecordingDetail,
  RecordingMeta,
  Role,
  ScoreView,
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

/** The named field of an unknown value, only if it really is a string.
 *
 *  `error.error` is typed `any` and holds whatever came back on the wire. When
 *  the backend answered it is the generated `ErrorBody`, but an ingress 502 sends
 *  HTML and a proxy can send a differently-shaped JSON — so asserting
 *  `Partial<ErrorBody>` onto it manufactured a `string` the compiler then trusted
 *  all the way to the screen, where a non-string paints as "[object Object]".
 *  A predicate, not an assertion, so the belief is earned rather than declared. */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
function stringField(value: unknown, key: string): string | null {
  if (!isRecord(value)) return null;
  const field = value[key];
  return typeof field === "string" && field !== "" ? field : null;
}

/**
 * Turn anything thrown by HttpClient into an {@link ApiFailure}.
 *
 * Exported so it can be tested directly, and so a future caller outside this
 * service classifies the same way rather than inventing a second scheme.
 */
export function classifyApiError(error: unknown): ApiFailure {
  if (!(error instanceof HttpErrorResponse)) {
    // NOT String(error): a thrown plain object stringifies to "[object Object]",
    // which is what the user would then be shown as the explanation.
    const message = error instanceof Error ? error.message : "something went wrong";
    return { kind: "unknown", message };
  }

  // Status 0 means the request never got an answer: the backend is not running,
  // or the network is gone. It is emphatically not "the request was refused".
  if (error.status === 0) {
    return { kind: "offline", message: "the backend is not responding" };
  }

  const code = stringField(error.error, "code");
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
   * Store a take.
   *
   * `role` says whether it defines the speaker or is only something to render,
   * and defaults to material — the safe direction. Only the guided calibration
   * flow sends `calibration`, because an upload that did not say what it was for
   * must not start shaping the sound world: this store fills up with other
   * people's singing, and a vowel space pooled across a crowd belongs to nobody.
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
   * Say what an already-stored take is for.
   *
   * Separate from `upload` because the answer is not always known when the audio
   * arrives — and until this existed it could never be changed afterwards, which
   * meant a take could not *become* the calibration one. Every recording made
   * before the distinction existed reads back as material, so a store that
   * predated it held the guided vowels and could not use them.
   */
  setRole(id: string, role: Role): Observable<RecordingMeta> {
    return this.http
      .put<RecordingMeta>(`/api/recordings/${id}/role`, { role })
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

  /**
   * What a render is made of, for the same parameters the render takes.
   *
   * The streams rather than the audio: the question a comparison asks is which
   * knob changed what, and that is legible in the score and buried in a
   * waveform.
   */
  score(id: string, query = ""): Observable<ScoreView> {
    return this.http
      .get<ScoreView>(`/api/recordings/${id}/score${suffix(query)}`)
      .pipe(catchError(rethrow));
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
