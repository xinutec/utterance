import { DecimalPipe } from "@angular/common";
import { ChangeDetectionStrategy, Component, computed, inject, input, model } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatButtonToggleModule } from "@angular/material/button-toggle";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatSelectModule } from "@angular/material/select";
import { MatSliderModule } from "@angular/material/slider";
import { MatTooltipModule } from "@angular/material/tooltip";

import type { Knob, MappingChoice } from "../../models";
import { RecordingsStore } from "../../recordings-store";
import { knobValue, withKnob, type MappingSettings } from "./mapping-settings";

/**
 * The mapping's knobs, as things you can turn.
 *
 * Every one of these was reachable only by editing a URL, which meant the
 * person the music is for could not explore it — and exploring is how the open
 * questions in `docs/roadmap.md` get answered. Whether the speaker's own tuning
 * beats equal temperament is not something anyone can settle by argument; it is
 * something you settle by moving a slider and listening twice.
 *
 * **Nothing here is written down twice.** The sliders, their ranges and their
 * explanations come from `GET /api/controls`, which the mapping crate publishes.
 * Adding a knob to `utterance_mapping::params::KNOBS` makes it appear here; changing
 * a range changes the slider. A UI keeping its own copy would eventually offer a
 * value the mapping clamps away, and the person turning it would hear nothing
 * and conclude the knob was broken.
 *
 * Moving anything changes only what the *next* render will be. Nothing here
 * fires a request: a render is seconds of backend work, and a slider that
 * re-rendered as it moved would queue a dozen renders per drag.
 */
@Component({
  selector: "app-mapping-controls",
  templateUrl: "./mapping-controls.html",
  styleUrl: "./mapping-controls.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    DecimalPipe,
    MatButtonModule,
    MatButtonToggleModule,
    MatFormFieldModule,
    MatSelectModule,
    MatSliderModule,
    MatTooltipModule,
  ],
})
export class MappingControls {
  /** The takes, for choosing which one the scale is derived from. */
  readonly store = inject(RecordingsStore);

  /** What the backend says it accepts. Fetched by the parent, which also needs it. */
  readonly knobs = input.required<readonly Knob[]>();
  readonly mappings = input.required<readonly MappingChoice[]>();

  /** The choices, owned by the parent so it can build the render URL from them. */
  readonly settings = model.required<MappingSettings>();

  /** Which mapping names are on, as the toggle group wants them. */
  readonly chosenMappings = computed(() => [...this.settings().mapping]);

  /**
   * The knobs the mappings being played actually read.
   *
   * A knob declares which mappings it reaches and an empty list means all of
   * them, so this filter needs no list of its own — the same arrangement as the
   * ranges. Showing one that the playing mapping ignores produces exactly the
   * failure the whole table exists to prevent: a slider that moves, and changes
   * nothing, and leaves the person turning it to conclude the thing is broken.
   */
  readonly relevant = computed(() => {
    const playing = this.settings().mapping;
    return this.knobs().filter(
      (knob) => knob.mappings.length === 0 || knob.mappings.some((m) => playing.includes(m)),
    );
  });

  /** True once anything has been moved, so the offer to reset means something. */
  readonly touched = computed(() => {
    const settings = this.settings();
    return Object.keys(settings.knobs).length > 0 || settings.calibration !== null;
  });

  value(knob: Knob): number {
    return knobValue(this.settings(), knob);
  }

  setKnob(knob: Knob, value: number): void {
    this.settings.set(withKnob(this.settings(), knob, value));
  }

  /**
   * Choose the mappings to hear.
   *
   * An empty choice is refused rather than sent: the backend has nothing to
   * render from it, and a toggle group with nothing on is a person mid-thought
   * rather than a person asking for silence.
   *
   * **Two mappings making the same material cannot both sound**, so turning one
   * on turns its rival off. The render route refuses the pair, and letting
   * someone assemble a refused combination and press play would teach them that
   * the page is broken rather than that the combination is meaningless. Which
   * mappings are rivals comes from `makes` in what the backend published, so a
   * fourth mapping needs no change here.
   */
  setMappings(names: string[]): void {
    if (names.length === 0) return;
    const before = this.settings().mapping;
    const added = names.filter((n) => !before.includes(n));
    const kept = names.filter(
      (name) => added.includes(name) || !added.some((a) => this.rivals(a, name)),
    );
    this.settings.set({ ...this.settings(), mapping: kept.length > 0 ? kept : names });
  }

  /** Whether two mappings compete to be the same part of the score. */
  private rivals(a: string, b: string): boolean {
    if (a === b) return false;
    const makes = (name: string) => this.mappings().find((m) => m.name === name)?.makes;
    const theirs = makes(a);
    return theirs !== undefined && theirs === makes(b);
  }

  setCalibration(id: string | null): void {
    this.settings.set({ ...this.settings(), calibration: id });
  }

  reset(): void {
    this.settings.set({ ...this.settings(), calibration: null, knobs: {} });
  }
}
