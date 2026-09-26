/**
 * The sign-in wall, raised by what the backend answers rather than by a check
 * of its own: with no gate configured, nothing ever answers 401 and the wall
 * never exists, so the frontend need not know which deployment it is.
 */

import type { HttpInterceptorFn } from "@angular/common/http";
import { Injectable, inject, signal } from "@angular/core";
import { tap } from "rxjs/operators";

import type { ErrorCode } from "./models";
import { classifyApiError } from "./recordings-api";

/** The backend's codes for the two ways in can be refused, typed as `ErrorCode`. */
const NOT_AUTHENTICATED: ErrorCode = "not_authenticated";
const NOT_PERMITTED: ErrorCode = "not_permitted";

@Injectable({ providedIn: "root" })
export class AuthState {
  /** True once the backend has said a request needed a session and had none. */
  readonly needsSignIn = signal(false);

  /**
   * Set when a signed-in Nextcloud user is not on this app's list — apart from
   * {@link needsSignIn}, since signing in again cannot fix it.
   */
  readonly refused = signal<string | null>(null);

  /** Where to send the browser, remembering the page it was on. */
  signInUrl(): string {
    const here = window.location.pathname + window.location.search;
    return `/login?return_to=${encodeURIComponent(here)}`;
  }
}

/**
 * Turn the backend's refusals into the wall, classified by `recordings-api` —
 * a proxy's HTML 401 has no `code`. Only these two codes raise it; any other 401
 * or 403 is an ordinary error, not a reason to sign in again.
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
