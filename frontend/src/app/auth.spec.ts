/**
 * The sign-in wall's trigger.
 *
 * Worth testing on its own because the wall is raised by a side effect of an
 * unrelated request failing, and the two ways to get that wrong are silent: a
 * wall that never appears leaves someone staring at an app where nothing loads,
 * and a wall raised by any old error sends them to sign in about something
 * signing in cannot fix.
 */

import { HttpErrorResponse, type HttpHandlerFn, type HttpRequest } from "@angular/common/http";
import { TestBed } from "@angular/core/testing";
import { throwError } from "rxjs";
import { beforeEach, describe, expect, it } from "vitest";

import { AuthState, authInterceptor } from "./auth";

/** An error shaped the way the backend really answers: a JSON `ErrorBody`. */
function refusal(status: number, code: string, message: string): HttpErrorResponse {
  return new HttpErrorResponse({
    status,
    url: "/api/controls",
    error: { code, message },
  });
}

/** Run the interceptor over a request whose response fails with `error`. */
function intercept(error: unknown): AuthState {
  const auth = TestBed.inject(AuthState);
  const request = { url: "/api/controls" } as HttpRequest<unknown>;
  const next: HttpHandlerFn = () => throwError(() => error);
  TestBed.runInInjectionContext(() => {
    authInterceptor(request, next).subscribe({
      // Swallowed: the interceptor observes the failure and passes it on, and
      // an unhandled rejection here would fail the test for the wrong reason.
      error: () => undefined,
    });
  });
  return auth;
}

describe("the sign-in wall", () => {
  beforeEach(() => {
    TestBed.configureTestingModule({});
  });

  it("goes up when the backend says a session was needed", () => {
    const auth = intercept(refusal(401, "not_authenticated", "sign in to continue"));
    expect(auth.needsSignIn()).toBe(true);
    expect(auth.refused()).toBeNull();
  });

  it("says so when the account is real but not on the list", () => {
    // A different message and a different remedy: signing in again with the
    // same account will fail identically.
    const auth = intercept(refusal(403, "not_permitted", "someone is not on the list"));
    expect(auth.refused()).toBe("someone is not on the list");
    expect(auth.needsSignIn()).toBe(false);
  });

  it("stays down for failures signing in cannot fix", () => {
    for (const error of [
      refusal(404, "not_found", "no such recording"),
      refusal(422, "unplayable", "Lattice cannot be played in this scale"),
      refusal(500, "storage_io", "disk"),
      new HttpErrorResponse({ status: 0 }),
      new Error("something else entirely"),
    ]) {
      TestBed.resetTestingModule();
      TestBed.configureTestingModule({});
      const auth = intercept(error);
      expect(auth.needsSignIn(), JSON.stringify(error)).toBe(false);
      expect(auth.refused(), JSON.stringify(error)).toBeNull();
    }
  });

  it("comes back to the page the person was on", () => {
    const auth = TestBed.inject(AuthState);
    // The value has to be encoded, or a path with a query string would end the
    // return_to parameter early and land them somewhere else.
    window.history.replaceState({}, "", "/compare?a=1");
    expect(auth.signInUrl()).toBe("/login?return_to=%2Fcompare%3Fa%3D1");
  });
});
