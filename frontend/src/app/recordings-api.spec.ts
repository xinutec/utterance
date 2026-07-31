/**
 * The boundary where a failure stops being HTTP and becomes something to say.
 *
 * Worth testing directly, and it was not until now: `classifyApiError` is the
 * one function every page's error message goes through, and every branch of it
 * is about a case nobody hits while developing — the backend down, a proxy
 * answering instead of the app, a code from a build that is not this one.
 */

import { HttpErrorResponse } from "@angular/common/http";
import { describe, expect, it } from "vitest";

import type { ErrorCode } from "./models";
import { classifyApiError } from "./recordings-api";

/** An error shaped the way the backend really answers: a JSON `ErrorBody`. */
function answered(status: number, body: unknown): HttpErrorResponse {
  return new HttpErrorResponse({ status, url: "/api/voice", error: body });
}

describe("classifyApiError", () => {
  it("tells nothing answering apart from being refused", () => {
    // Status 0 is the backend not running. Calling that a refusal would tell
    // someone their recording was rejected when nothing ever heard of it.
    expect(classifyApiError(answered(0, null))).toEqual({
      kind: "offline",
      message: "the backend is not responding",
    });
  });

  it("keeps a code the backend defines, and says whose fault it is", () => {
    const rejected = classifyApiError(answered(422, { code: "unplayable", message: "no plane" }));
    expect(rejected).toEqual({ kind: "rejected", code: "unplayable", message: "no plane" });

    const server = classifyApiError(answered(500, { code: "storage_io", message: "disk" }));
    expect(server).toEqual({ kind: "server", code: "storage_io", message: "disk" });
  });

  it("refuses to believe a code it does not know", () => {
    // The frontend is served by the backend that produces these codes, so an
    // unrecognised one is a bug rather than version skew — and claiming it as an
    // `ErrorCode` would let a page branch on a value the type says cannot exist.
    // The message still gets through, which is all that can honestly be said.
    const parsed = classifyApiError(answered(400, { code: "not_a_code", message: "hmm" }));
    expect(parsed).toEqual({ kind: "unknown", message: "hmm" });
  });

  it("survives a body that is not an error body at all", () => {
    // An ingress 502 answers with HTML, and a proxy can send differently-shaped
    // JSON. Reading `.code` off either used to manufacture a value the compiler
    // then trusted all the way to the screen.
    expect(classifyApiError(answered(502, "<html>Bad Gateway</html>")).kind).toBe("unknown");
    expect(classifyApiError(answered(400, { code: 7, message: 9 })).kind).toBe("unknown");
  });

  it("does not stringify a thrown object into the explanation", () => {
    // `String({})` is "[object Object]", which is what a person would be shown.
    expect(classifyApiError({ nope: true })).toEqual({
      kind: "unknown",
      message: "something went wrong",
    });
    expect(classifyApiError(new Error("boom"))).toEqual({ kind: "unknown", message: "boom" });
  });

  it("accepts every code the backend declares", () => {
    // Walks the generated union, so a code added in Rust that nobody taught this
    // module about fails here rather than reaching a page as "unknown".
    const codes: ErrorCode[] = [
      "audio_undecodable",
      "audio_empty",
      "audio_too_short",
      "not_found",
      "record_corrupt",
      "storage_io",
      "bad_request",
      "unplayable",
      "no_calibration",
      "not_authenticated",
      "not_permitted",
      "bad_login_state",
      "no_authorization_code",
      "sign_in_failed",
    ];
    for (const code of codes) {
      const failure = classifyApiError(answered(400, { code, message: "x" }));
      expect(failure).toMatchObject({ code });
    }
  });
});
