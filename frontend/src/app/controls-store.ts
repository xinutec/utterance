import { Injectable, inject, signal } from "@angular/core";

import type { Knob, MappingChoice } from "./models";
import { RecordingsApi } from "./recordings-api";

/**
 * What the mapping says it can be asked for.
 *
 * Root-provided and fetched once, like the take list next door and for the same
 * reason: a component that fetches its own list empties it every time the
 * component is destroyed, so switching tabs blanks the sliders and re-requests
 * them. This list is stronger than that even — it cannot change while the page
 * is open, because it is a property of the running backend rather than of
 * anything anyone does here.
 *
 * Failure is deliberately quiet. Without these the studio still renders at the
 * mapping's defaults, which is exactly what it did before there were any
 * controls; only the sliders are missing, and an error banner over a working
 * player would misdescribe that.
 */
@Injectable({ providedIn: "root" })
export class ControlsStore {
  private readonly api = inject(RecordingsApi);

  readonly knobs = signal<readonly Knob[]>([]);
  readonly mappings = signal<readonly MappingChoice[]>([]);

  /** Guards against a second request while the first is still in flight. */
  private asked = false;

  /** Fetch them if they have not been fetched. Safe to call from any component. */
  ensure(): void {
    if (this.asked) return;
    this.asked = true;
    this.api.controls().subscribe({
      next: (controls) => {
        this.knobs.set(controls.knobs);
        this.mappings.set(controls.mappings);
      },
      // Allow a later attempt: the backend may simply not have been up yet.
      error: () => {
        this.asked = false;
      },
    });
  }
}
