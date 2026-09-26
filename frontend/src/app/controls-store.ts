import { Injectable, inject, signal } from "@angular/core";

import type { Knob, MappingChoice } from "./models";
import { RecordingsApi } from "./recordings-api";

/**
 * What the mapping says it can be asked for, root-provided and fetched once: it
 * cannot change while the page is open. Failure is quiet — the studio still
 * works at the defaults, just without sliders.
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
