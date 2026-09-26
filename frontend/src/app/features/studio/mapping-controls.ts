import { DecimalPipe, NgTemplateOutlet } from "@angular/common";
import { ChangeDetectionStrategy, Component, computed, inject, input, model } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatButtonToggleModule } from "@angular/material/button-toggle";
import { MatExpansionModule } from "@angular/material/expansion";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatSelectModule } from "@angular/material/select";
import { MatSliderModule } from "@angular/material/slider";
import { MatTooltipModule } from "@angular/material/tooltip";

import type { Knob, Mapping, MappingChoice } from "../../models";
import { RecordingsStore } from "../../recordings-store";
import { knobValue, withKnob, type MappingSettings } from "./mapping-settings";

/**
 * The mapping's knobs, as things you can turn — the open questions are settled
 * by moving a slider and listening, not by editing URLs.
 *
 * The sliders, ranges and explanations come from `GET /api/controls`, so a knob
 * added in Rust appears here and a slider cannot offer a value the mapping
 * clamps away. Moving one changes only the *next* render; nothing is fetched on
 * a drag.
 */
@Component({
  selector: "app-mapping-controls",
  templateUrl: "./mapping-controls.html",
  styleUrl: "./mapping-controls.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    DecimalPipe,
    NgTemplateOutlet,
    MatButtonModule,
    MatButtonToggleModule,
    MatExpansionModule,
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
   * The knobs the playing mappings actually read, from each knob's own list, so
   * no slider is shown that moves and changes nothing.
   */
  readonly relevant = computed(() => {
    const playing = this.settings().mapping;
    return this.knobs().filter(
      (knob) => knob.mappings.length === 0 || knob.mappings.some((m) => playing.includes(m)),
    );
  });

  /**
   * Primary knobs, then the rest — the split arrives on each knob, so there is
   * no second list here to drift.
   */
  readonly primary = computed(() => this.relevant().filter((knob) => knob.primary));
  readonly advanced = computed(() => this.relevant().filter((knob) => !knob.primary));

  /**
   * How many advanced knobs have been moved, for the folded panel's label, so a
   * closed panel does not hide a moved knob.
   */
  readonly advancedMoved = computed(() => {
    const moved = this.settings().knobs;
    return this.advanced().filter((knob) => moved[knob.name] !== undefined).length;
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
   * Choose the mappings to hear. An empty choice is ignored: it is a person
   * mid-thought, not a request for silence. Two mappings making the same
   * material cannot sound together, so turning one on turns its rival off, going
   * by what the backend says each makes.
   */
  setMappings(names: Mapping[]): void {
    if (names.length === 0) return;
    const before = this.settings().mapping;
    const added = names.filter((n) => !before.includes(n));
    const kept = names.filter(
      (name) => added.includes(name) || !added.some((a) => this.rivals(a, name)),
    );
    this.settings.set({ ...this.settings(), mapping: kept.length > 0 ? kept : names });
  }

  /** Whether two mappings compete to be the same part of the score. */
  private rivals(a: Mapping, b: Mapping): boolean {
    if (a === b) return false;
    const makes = (name: Mapping) => this.mappings().find((m) => m.name === name)?.makes;
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
