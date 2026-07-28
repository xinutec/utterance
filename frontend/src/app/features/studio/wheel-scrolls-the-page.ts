import { DestroyRef, Directive, ElementRef, inject } from "@angular/core";

/**
 * A slider that is scrolled past stays where it was.
 *
 * **The behaviour this removes.** A native range input responds to the wheel in
 * several browsers: put the pointer anywhere over one, scroll the page, and its
 * value moves. On a page with nine sliders in a column that is not an edge case
 * — it is what happens every time someone reads down the list. The setting that
 * changes is silent, it is whichever slider the pointer happened to cross, and
 * the person had no reason to look. A control that a reader can alter without
 * intending to is worse than one that is hard to reach.
 *
 * **Why it also scrolls the page.** Cancelling the wheel is what stops the value
 * moving, and it stops the scroll with it — so the pointer resting on a slider
 * would freeze the page, which is the same complaint from the other direction.
 * The wheel means *scroll*, so this does exactly that and nothing else. The
 * document is the only scroller on these pages; a nested one would need finding
 * first, and there is no need to write that until there is one.
 *
 * Deliberately not a knob and not configurable: nobody wants the other
 * behaviour. It applies to every slider by selector, so a slider added anywhere
 * inherits it.
 */
@Directive({ selector: "input[matSliderThumb]" })
export class WheelScrollsThePage {
  constructor() {
    const input = inject<ElementRef<HTMLInputElement>>(ElementRef).nativeElement;
    const scroll = (event: WheelEvent) => {
      // Cancelling is the whole point, so the listener cannot be passive.
      event.preventDefault();
      window.scrollBy(0, pixels(event));
    };

    input.addEventListener("wheel", scroll, { passive: false });
    inject(DestroyRef).onDestroy(() => input.removeEventListener("wheel", scroll));
  }
}

/**
 * How far a wheel event means to scroll, in pixels.
 *
 * A wheel reports its distance in one of three units and the browser chooses
 * which: Firefox sends lines where Chrome sends pixels. Reading `deltaY` and
 * assuming pixels is the bug where one browser scrolls a fifteenth as far as the
 * other and the page feels stuck.
 */
function pixels(event: WheelEvent): number {
  if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) return event.deltaY * LINE_HEIGHT;
  if (event.deltaMode === WheelEvent.DOM_DELTA_PAGE) return event.deltaY * window.innerHeight;
  return event.deltaY;
}

/** Pixels in a line, for the browsers that measure the wheel in lines. */
const LINE_HEIGHT = 16;
