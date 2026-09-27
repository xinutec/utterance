/**
 * The store's policy — what an upload opens, which failures cover the screen,
 * what a delete invalidates — none of it visible in a type.
 */

import { TestBed } from "@angular/core/testing";
import { Observable, of, throwError } from "rxjs";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { RecordingDetail, RecordingMeta, Role, SpeakerCorner, SpeakerCorners } from "./models";
import { ApiError, RecordingsApi } from "./recordings-api";
import { RecordingsStore } from "./recordings-store";
import { Feedback } from "./shared/feedback";

function meta(id: string, role: Role = "material"): RecordingMeta {
  return {
    id,
    label: id,
    createdAtMs: 0,
    durationS: 9,
    sampleRateHz: 48000,
    voicedFraction: 0.7,
    onsetCount: 12,
    peak: 0.5,
    clipped: false,
    role,
  };
}

/** Only `meta` is read by anything here, so the voiceprint is left unbuilt. */
function detail(m: RecordingMeta): RecordingDetail {
  return { meta: m, voiceprint: undefined as unknown as RecordingDetail["voiceprint"] };
}

function corner(step: SpeakerCorner["step"]): SpeakerCorner {
  return {
    step,
    corner: "open",
    f1Hz: 626,
    f2Hz: 1240,
    f1SpreadHz: 17,
    f2SpreadHz: 46,
    frames: 390,
  };
}

/**
 * A stand-in for the HTTP service, answering synchronously: the store has no
 * timers of its own, so nothing depends on the delay.
 */
function defaults() {
  return {
    list: vi.fn(() => of([meta("a"), meta("b")])),
    get: vi.fn((id: string) => of(detail(meta(id)))),
    speakerCorners: vi.fn((): Observable<SpeakerCorners> => of({ corners: [corner("vowel-ah")] })),
    upload: vi.fn(() => of(detail(meta("new")))),
    setRole: vi.fn(() => of(meta("a", "calibration"))),
    delete: vi.fn(() => of({ id: "a" })),
    audioUrl: vi.fn((id: string) => `/api/recordings/${id}/audio`),
  };
}

type ApiStub = ReturnType<typeof defaults>;

function api(overrides: Partial<ApiStub> = {}): ApiStub {
  return { ...defaults(), ...overrides };
}

/** The undo bar, held open: the test decides whether it is tapped or times out. */
function undoBar() {
  const offered: { message: string; onUndo: () => void; onCommit: () => void }[] = [];
  const feedback = {
    undo: vi.fn((message: string, onUndo: () => void, onCommit: () => void) => {
      offered.push({ message, onUndo, onCommit });
    }),
  };
  return { feedback, offered };
}

function storeWith(stub: ApiStub, bar = undoBar()): RecordingsStore {
  TestBed.configureTestingModule({
    providers: [
      { provide: RecordingsApi, useValue: stub },
      { provide: Feedback, useValue: bar.feedback },
    ],
  });
  return TestBed.inject(RecordingsStore);
}

/** A failure classified the way `recordings-api` really rejects. */
function refusal(kind: "offline" | "rejected" | "server", message: string): Observable<never> {
  const failure =
    kind === "offline"
      ? ({ kind, message } as const)
      : ({ kind, code: "storage_io", message } as const);
  return throwError(() => new ApiError(failure));
}

beforeEach(() => {
  TestBed.resetTestingModule();
});

describe("opening a take", () => {
  // A take has its own screen now, so the list opens nothing by itself.
  it("opens nothing on a refresh", () => {
    const store = storeWith(api());
    store.refresh();
    expect(store.selected()).toBeNull();
  });

  it("opens the take it is asked for, by id", () => {
    const stub = api();
    const store = storeWith(stub);
    store.open("b");
    expect(stub.get).toHaveBeenCalledWith("b");
    expect(store.selected()?.meta.id).toBe("b");
  });

  it("says which take an upload stored, so the page can open it", () => {
    const store = storeWith(api());
    let stored: string | undefined;
    store.upload(new Blob(), "take", "material", (id) => {
      stored = id;
    });
    expect(stored).toBe("new");
  });
});

describe("what a failure is allowed to cover the screen with", () => {
  it("raises the take list's failure, which is the page not working", () => {
    const store = storeWith(api({ list: vi.fn(() => refusal("offline", "nope")) }));
    store.refresh();
    expect(store.error()).toBe("the backend is not responding — is `scripts/dev.sh` still running?");
  });

  it("swallows the corners' failure and keeps the ones it had", () => {
    // The corners' failure is not worth a banner; the chart says it is generic.
    const stub = api();
    const store = storeWith(stub);
    store.refresh();
    expect(store.corners()).toHaveLength(1);

    stub.speakerCorners.mockImplementation(() => refusal("server", "disk"));
    store.refresh();
    expect(store.error()).toBeNull();
    expect(store.corners()).toHaveLength(1);
  });

  it("stops waiting when a request fails", () => {
    // A store left busy is a spinner over an error, with every button disabled.
    const store = storeWith(api({ get: vi.fn(() => refusal("rejected", "too short")) }));
    store.open("a");
    expect(store.busy()).toBe(false);
    expect(store.error()).toBe("too short");
  });

  it("names the code for a fault that is the backend's own", () => {
    const store = storeWith(api({ upload: vi.fn(() => refusal("server", "disk")) }));
    store.upload(new Blob(), "take");
    expect(store.error()).toBe("the backend failed to handle that (storage_io) — check its log");
  });
});

describe("deleting", () => {
  it("closes the take that was deleted", () => {
    const store = storeWith(api());
    store.open("a");
    store.remove(meta("a"));
    expect(store.selected()).toBeNull();
  });

  it("leaves a different take open", () => {
    const store = storeWith(api());
    store.open("b");
    store.remove(meta("a"));
    expect(store.selected()?.meta.id).toBe("b");
  });

  // The audio is the one thing that cannot be re-derived, and the bin sits
  // beside the voice toggle.
  it("hides the take at once but deletes nothing while Undo is offered", () => {
    const stub = api();
    const bar = undoBar();
    const store = storeWith(stub, bar);
    store.refresh();
    store.remove(meta("a"));
    expect(store.recordings().map((r) => r.id)).toEqual(["b"]);
    expect(stub.delete).not.toHaveBeenCalled();
    expect(bar.offered[0]?.message).toBe("Deleted a");
  });

  it("brings the take back on Undo, and never deletes it", () => {
    const stub = api();
    const bar = undoBar();
    const store = storeWith(stub, bar);
    store.refresh();
    store.remove(meta("a"));
    bar.offered[0]?.onUndo();
    expect(store.recordings().map((r) => r.id)).toEqual(["a", "b"]);
    expect(stub.delete).not.toHaveBeenCalled();
  });

  it("deletes on the server once the bar closes without Undo", () => {
    const stub = api();
    const bar = undoBar();
    const store = storeWith(stub, bar);
    store.refresh();
    store.remove(meta("a"));
    stub.list.mockImplementation(() => of([meta("b")]));
    bar.offered[0]?.onCommit();
    expect(stub.delete).toHaveBeenCalledWith("a");
    expect(store.recordings().map((r) => r.id)).toEqual(["b"]);
  });

  it("does not flash the take back while the list reloads", () => {
    const stub = api();
    const bar = undoBar();
    const store = storeWith(stub, bar);
    store.refresh();
    store.remove(meta("a"));
    // The reload never answers, so only the local drop can hide it.
    stub.list.mockImplementation(() => new Observable<RecordingMeta[]>());
    bar.offered[0]?.onCommit();
    expect(store.recordings().map((r) => r.id)).toEqual(["b"]);
  });

  it("brings the take back, and says so, when the server refuses the delete", () => {
    const bar = undoBar();
    const store = storeWith(api({ delete: vi.fn(() => refusal("server", "disk")) }), bar);
    store.refresh();
    store.remove(meta("a"));
    bar.offered[0]?.onCommit();
    expect(store.recordings().map((r) => r.id)).toEqual(["a", "b"]);
    expect(store.error()).toContain("storage_io");
  });
});

describe("changing what a take is for", () => {
  it("re-reads the open take, because its role changed what it means", () => {
    // The role changes the scale, vowel space and tonic, so the list reloads.
    const stub = api();
    const store = storeWith(stub);
    store.open("a");
    stub.get.mockClear();
    store.setRole(meta("a"), "calibration");
    expect(stub.get).toHaveBeenCalledWith("a");
  });

  it("re-reads the corners, since they are derived from the calibration takes", () => {
    const stub = api();
    const store = storeWith(stub);
    stub.speakerCorners.mockClear();
    store.setRole(meta("a"), "calibration");
    expect(stub.speakerCorners).toHaveBeenCalled();
  });
});
