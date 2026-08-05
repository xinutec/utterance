/**
 * The two capture seams, which are the only part of the trace this app owns.
 *
 * The queue, the flush cadence, the transport and the label rules belong to
 * `@xinutec/ui-harness/telemetry` and are tested there. What is left here is a
 * dozen lines of wiring, and every one of them fails silently: a trace is
 * best-effort by contract, so a seam that stopped firing would show up as an
 * activity log that quietly went thin rather than as anything breaking.
 *
 * `TelemetryCore` is spied at the prototype rather than injected, because the
 * adapter constructs its own — deliberately, so nothing in the app can reach the
 * queue. That makes the prototype the only seam a test has, and it is the right
 * one: it asserts what this class *asks the core to do*, which is the whole of
 * its behaviour.
 */

import { TestBed } from "@angular/core/testing";
import { NavigationEnd, NavigationStart, Router } from "@angular/router";
import { TelemetryCore } from "@xinutec/ui-harness/telemetry";
import { Subject } from "rxjs";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Telemetry } from "./telemetry";

function setUp(url = "/studio") {
  const events = new Subject<unknown>();
  const router = { events: events.asObservable(), url };
  TestBed.configureTestingModule({ providers: [{ provide: Router, useValue: router }] });

  const record = vi.spyOn(TelemetryCore.prototype, "record").mockImplementation(() => undefined);
  const recordTap = vi.spyOn(TelemetryCore.prototype, "recordTap").mockImplementation(() => undefined);

  // The real `start` is stubbed so no flush timer outlives the test, but
  // `started` has to keep answering the way the real one does — it is defined as
  // "whether `start()` has already run", and it is what makes `init()`
  // idempotent. A stub that always said false would leave the guard untested and
  // the seams wired twice, which is what the first run of this spec did.
  let started = false;
  const start = vi.spyOn(TelemetryCore.prototype, "start").mockImplementation(() => {
    started = true;
  });
  vi.spyOn(TelemetryCore.prototype, "started", "get").mockImplementation(() => started);

  return { events, router, record, recordTap, start, telemetry: TestBed.inject(Telemetry) };
}

beforeEach(() => {
  TestBed.resetTestingModule();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("wiring the seams", () => {
  it("starts the core", () => {
    const { telemetry, start } = setUp();
    telemetry.init();
    expect(start).toHaveBeenCalledOnce();
  });

  it("wires nothing a second time", () => {
    // The app shell calls this on every bootstrap, and a hot reload bootstraps
    // again into the same document. Two click listeners would count one tap
    // twice, which is worse than counting none — a silent doubling reads as
    // real activity.
    const { telemetry, start } = setUp();
    const listen = vi.spyOn(document, "addEventListener");
    telemetry.init();
    const afterFirst = listen.mock.calls.filter(([type]) => type === "click").length;

    telemetry.init();
    const afterSecond = listen.mock.calls.filter(([type]) => type === "click").length;

    expect(afterFirst).toBe(1);
    expect(afterSecond).toBe(1);
    expect(start).toHaveBeenCalledOnce();
  });
});

describe("navigation", () => {
  it("records where the router actually landed, not where it was sent", () => {
    // `url` is what was requested and `urlAfterRedirects` is what was served.
    // A guard sending a signed-out visitor to the sign-in page would otherwise
    // log the page they never saw.
    const { telemetry, events, record } = setUp();
    telemetry.init();
    events.next(new NavigationEnd(1, "/studio", "/login"));
    expect(record).toHaveBeenCalledWith("nav", "/login", null);
  });

  it("ignores the events that are not an arrival", () => {
    // The router emits a dozen event types per navigation. Recording the stream
    // rather than filtering it would log every one of them as a page view.
    const { telemetry, events, record } = setUp();
    telemetry.init();
    events.next(new NavigationStart(1, "/studio"));
    expect(record).not.toHaveBeenCalled();
  });

  it("records nothing before init", () => {
    const { events, record } = setUp();
    events.next(new NavigationEnd(1, "/studio", "/studio"));
    expect(record).not.toHaveBeenCalled();
  });
});

describe("taps", () => {
  it("records the tap against the page it happened on", () => {
    const { telemetry, recordTap } = setUp("/compare");
    telemetry.init();
    const button = document.createElement("button");
    document.body.appendChild(button);
    button.click();
    expect(recordTap).toHaveBeenCalledWith(button, "/compare");
    button.remove();
  });

  it("sees a tap whose handler stops it reaching anyone else", () => {
    // Registered on the capture phase for exactly this. A control that calls
    // `stopPropagation` — which several Material components do — would be
    // invisible to a bubble-phase listener, so the most-used buttons in the app
    // would be the ones missing from the trace.
    const { telemetry, recordTap } = setUp();
    telemetry.init();
    const button = document.createElement("button");
    button.addEventListener("click", (ev) => {
      ev.stopPropagation();
    });
    document.body.appendChild(button);
    button.click();
    expect(recordTap).toHaveBeenCalledWith(button, "/studio");
    button.remove();
  });
});
