import { ElementRef } from "@angular/core";
import { TestBed } from "@angular/core/testing";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { WheelScrollsThePage } from "./wheel-scrolls-the-page";

/**
 * The directive attached to a bare range input.
 *
 * Built by hand rather than through a host component, because a host would need
 * an inline template and this project keeps templates in files — and because the
 * directive's whole content is what it does to one element's wheel events, which
 * a host would only wrap.
 */
function thumb(): HTMLInputElement {
  const input = document.createElement("input");
  input.type = "range";
  TestBed.configureTestingModule({
    providers: [{ provide: ElementRef, useValue: new ElementRef(input) }],
  });
  TestBed.runInInjectionContext(() => new WheelScrollsThePage());
  return input;
}

function wheel(deltaY: number, deltaMode = 0): WheelEvent {
  return new WheelEvent("wheel", { deltaY, deltaMode, bubbles: true, cancelable: true });
}

describe("a slider under the wheel", () => {
  let scrolled: number[];

  beforeEach(() => {
    TestBed.resetTestingModule();
    scrolled = [];
    // jsdom has no layout, so `window.scrollBy` moves nothing there and the only
    // thing worth asserting is what it was asked for.
    vi.spyOn(window, "scrollBy").mockImplementation((...args: unknown[]) => {
      scrolled.push(args[1] as number);
    });
  });

  it("refuses the wheel rather than letting it move the value", () => {
    // The browser only adjusts a range input if its wheel event survives, so
    // cancelling is the whole mechanism. Asserting on the *value* instead would
    // test jsdom, which never had the behaviour to begin with.
    const event = wheel(120);
    thumb().dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });

  it("scrolls the page by what the wheel asked for", () => {
    // Cancelling the wheel cancels the scroll with it, so a pointer resting on a
    // slider would freeze the page. This is the other half of the fix.
    thumb().dispatchEvent(wheel(120));
    expect(scrolled).toEqual([120]);
  });

  it("reads a wheel that counts in lines rather than pixels", () => {
    // Firefox measures the wheel in lines. Treating three lines as three pixels
    // is the bug where the page barely moves in one browser and not the other.
    thumb().dispatchEvent(wheel(3, WheelEvent.DOM_DELTA_LINE));
    expect(scrolled).toEqual([48]);
  });
});
