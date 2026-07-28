/**
 * The sign-in wall, driven by what the backend answers rather than by a check
 * of its own.
 *
 * There is deliberately no "am I logged in?" probe on startup. The gate is
 * absent on the Mac and in dev — the backend only raises it when Nextcloud
 * credentials are in its environment — so a frontend that asked would have to
 * understand two deployments. Instead every request is allowed to go out, and a
 * 401 is what raises the wall. Where there is no gate, nothing ever answers 401
 * and the wall never exists.
 */

import type { HttpInterceptorFn } from "@angular/common/http";
import { Injectable, inject, signal } from "@angular/core";
import { tap } from "rxjs/operators";

import { classifyApiError } from "./recordings-api";

/** The backend's stable codes for the two ways in can be refused. */
const NOT_AUTHENTICATED = "not_authenticated";
const NOT_PERMITTED = "not_permitted";

@Injectable({ providedIn: "root" })
export class AuthState {
  /** True once the backend has said a request needed a session and had none. */
  readonly needsSignIn = signal(false);

  /**
   * Set when a real Nextcloud user signed in and is not on this app's list.
   *
   * Held apart from {@link needsSignIn} because the two need opposite advice:
   * one is fixed by signing in, and the other is not fixed by anything the
   * person at the screen can do.
   */
  readonly refused = signal<string | null>(null);

  /** Where to send the browser, remembering the page it was on. */
  signInUrl(): string {
    const here = window.location.pathname + window.location.search;
    return `/login?return_to=${encodeURIComponent(here)}`;
  }
}

/**
 * Turn the backend's refusals into the wall.
 *
 * Classification is borrowed from `recordings-api` rather than repeated here:
 * `HttpErrorResponse.error` is typed `any` and holds whatever came back on the
 * wire, so reading `.code` off it directly is a belief rather than a check —
 * and a proxy's HTML 401 would then set a `code` of `undefined` and quietly do
 * nothing, or worse, something.
 *
 * Only these two codes are claimed. Any other 401 or 403 is left to be reported
 * as an ordinary error, because a wall raised by an unrelated failure sends
 * someone to sign in over and over about something signing in cannot fix.
 */
export const authInterceptor: HttpInterceptorFn = (request, next) => {
  const auth = inject(AuthState);
  return next(request).pipe(
    tap({
      error: (error: unknown) => {
        const failure = classifyApiError(error);
        if (!("code" in failure)) return;
        if (failure.code === NOT_AUTHENTICATED) {
          auth.needsSignIn.set(true);
        } else if (failure.code === NOT_PERMITTED) {
          auth.refused.set(failure.message);
        }
      },
    }),
  );
};
