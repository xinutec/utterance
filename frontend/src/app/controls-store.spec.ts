/**
 * Fetch-once, and what a failure is allowed to leave behind.
 *
 * There is no rendering here to cover this from above and no HTTP below it —
 * `recordings-api.spec` owns the wire, and this store's whole job is the two
 * decisions between: that the request happens exactly once no matter how many
 * components ask, and that a failure stays quiet while still leaving a retry
 * possible. Both are invisible in a type, and both fail silently.
 */

import { TestBed } from "@angular/core/testing";
import { type Observable, Subject, of, throwError } from "rxjs";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ControlsStore } from "./controls-store";
import type { Controls, Knob, MappingChoice } from "./models";
import { ApiError, RecordingsApi } from "./recordings-api";

function knob(name: Knob["name"]): Knob {
  return {
    name,
    label: name,
    min: 0,
    max: 1,
    step: 0.01,
    default: 0.5,
    about: "",
    mappings: [],
    primary: true,
  };
}

function mapping(name: MappingChoice["name"]): MappingChoice {
  return { name, label: name, makes: "texture", about: "" };
}

const CONTROLS: Controls = { knobs: [knob("bind")], mappings: [mapping("tonnetz")] };

/** A failure shaped the way `recordings-api` really rejects. */
function refusal(): Observable<never> {
  return throwError(() => new ApiError({ kind: "offline", message: "nothing there" }));
}

function storeWith(controls: () => Observable<Controls>) {
  const stub = { controls: vi.fn(controls) };
  TestBed.configureTestingModule({ providers: [{ provide: RecordingsApi, useValue: stub }] });
  return { store: TestBed.inject(ControlsStore), stub };
}

beforeEach(() => {
  TestBed.resetTestingModule();
});

describe("asking for the knob table", () => {
  it("holds what the mapping said it can be asked for", () => {
    const { store } = storeWith(() => of(CONTROLS));
    store.ensure();
    expect(store.knobs()).toEqual(CONTROLS.knobs);
    expect(store.mappings()).toEqual(CONTROLS.mappings);
  });

  it("asks once however many components ask it to", () => {
    // The table is a property of the running backend, so it cannot change while
    // the page is open. Re-fetching per component is what blanked the sliders on
    // every tab switch before this store existed.
    const { store, stub } = storeWith(() => of(CONTROLS));
    store.ensure();
    store.ensure();
    store.ensure();
    expect(stub.controls).toHaveBeenCalledTimes(1);
  });

  it("does not ask again while the first ask is still in flight", () => {
    // The guard has to be set before subscribing, not in the success handler:
    // three components calling this during one change detection pass all run
    // before any response arrives.
    const pending = new Subject<Controls>();
    const { store, stub } = storeWith(() => pending.asObservable());
    store.ensure();
    store.ensure();
    expect(stub.controls).toHaveBeenCalledTimes(1);

    pending.next(CONTROLS);
    expect(store.knobs()).toHaveLength(1);
  });
});

describe("when the backend is not there", () => {
  it("stays empty rather than throwing", () => {
    // Without the table the studio still renders at the mapping's defaults —
    // which is what it did before there were any controls. Only the sliders are
    // missing, so there is nothing here worth an error banner.
    const { store } = storeWith(refusal);
    expect(() => {
      store.ensure();
    }).not.toThrow();
    expect(store.knobs()).toEqual([]);
    expect(store.mappings()).toEqual([]);
  });

  it("lets a later attempt through, because the backend may not have been up yet", () => {
    // `scripts/dev.sh` starts the frontend before the backend has finished
    // compiling, so the first ask of a dev session routinely fails. A store that
    // latched on that failure would leave the sliders missing until a reload.
    let answer: () => Observable<Controls> = refusal;
    const { store, stub } = storeWith(() => answer());
    store.ensure();
    expect(store.knobs()).toEqual([]);

    answer = () => of(CONTROLS);
    store.ensure();
    expect(stub.controls).toHaveBeenCalledTimes(2);
    expect(store.knobs()).toEqual(CONTROLS.knobs);
  });

  it("stops asking once an attempt succeeds", () => {
    // The retry above must not turn into a request per caller for the rest of
    // the session.
    let answer: () => Observable<Controls> = refusal;
    const { store, stub } = storeWith(() => answer());
    store.ensure();
    answer = () => of(CONTROLS);
    store.ensure();
    store.ensure();
    expect(stub.controls).toHaveBeenCalledTimes(2);
  });
});
