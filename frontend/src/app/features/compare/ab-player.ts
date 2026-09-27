import { DecimalPipe } from "@angular/common";
import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  input,
  linkedSignal,
  model,
  output,
  viewChild,
} from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatButtonToggleModule } from "@angular/material/button-toggle";
import { MatIconModule } from "@angular/material/icon";
import { MatProgressBarModule } from "@angular/material/progress-bar";

/** Which side is audible. */
export type Side = "a" | "b";

/**
 * Two renders played at once with one muted, so switching sides is instant and
 * lands on the same moment of the piece. One element whose source swaps would
 * reload and reseek, and the gap defeats the comparison.
 */
@Component({
  selector: "app-ab-player",
  templateUrl: "./ab-player.html",
  styleUrl: "./ab-player.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [DecimalPipe, MatButtonModule, MatButtonToggleModule, MatIconModule, MatProgressBarModule],
})
export class AbPlayer {
  /** The two renders' URLs. */
  readonly a = input.required<string>();
  readonly b = input.required<string>();
  /** A moment worth jumping to, in seconds, offered as a button. */
  readonly jump = input<number | null>(null);

  /** Which side is audible. */
  readonly side = model<Side>("a");
  /** Where the audible player is, in seconds. The owner resets it on new renders. */
  readonly playhead = model(0);
  /** Whether a side has been given a new render it cannot play yet. */
  readonly loading = model(false);

  /** Why playback failed, in words. */
  readonly failed = output<string>();

  /** A new source stops an element, so new renders are not playing. */
  readonly playing = linkedSignal(() => {
    this.a();
    this.b();
    return false;
  });

  /**
   * Sides whose player has a new render it cannot play yet, cleared by the
   * element's own `canplay` or `error`. Derived on read rather than by an
   * effect, so the player never looks ready between new URLs and the effect.
   * Only a side whose URL changed is waited on: an unchanged one loads nothing
   * and never reports it can play.
   */
  private readonly waiting = linkedSignal<Record<Side, string>, ReadonlySet<Side>>({
    source: () => ({ a: this.a(), b: this.b() }),
    computation: (next, previous) => {
      const pending = new Set(previous?.value);
      if (next.a !== previous?.source.a) pending.add("a");
      if (next.b !== previous?.source.b) pending.add("b");
      return pending;
    },
  });
  readonly pending = computed(() => this.waiting().size > 0);

  private readonly playerA = viewChild<ElementRef<HTMLAudioElement>>("playerA");
  private readonly playerB = viewChild<ElementRef<HTMLAudioElement>>("playerB");

  constructor() {
    // For the owner's own controls, which may lag a tick; the player's don't.
    effect(() => {
      this.loading.set(this.pending());
    });
  }

  /** A player has enough of its render to start. */
  ready(side: Side): void {
    this.settle(side);
  }

  /** A player could not load its render. */
  broken(side: Side): void {
    this.settle(side);
    this.failed.emit(`the render for ${side.toUpperCase()} could not be loaded`);
  }

  async toggle(): Promise<void> {
    const players = this.both();
    if (players.length < 2) return;

    if (this.playing()) {
      players.forEach((p) => p.pause());
      this.playing.set(false);
      return;
    }

    this.applySide();
    // Started together. `play()` can be refused; say so rather than silently not.
    try {
      await Promise.all(players.map((p) => p.play()));
      this.playing.set(true);
    } catch (err: unknown) {
      this.failed.emit(err instanceof Error ? err.message : "the browser refused to play");
    }
  }

  /**
   * Switch which side is audible at the same instant, nudging the silent player
   * onto the audible one's clock first — they drift apart over a minute.
   */
  choose(side: Side): void {
    const from = this.audible();
    this.side.set(side);
    const to = this.audible();
    if (from && to && to !== from && Math.abs(to.currentTime - from.currentTime) > 0.02) {
      to.currentTime = from.currentTime;
    }
    this.applySide();
  }

  /**
   * Move both players to the same moment. A seek before metadata has loaded is
   * silently dropped, so it waits for `loadedmetadata`.
   */
  seekTo(seconds: number): void {
    for (const player of this.both()) {
      if (player.readyState >= HTMLMediaElement.HAVE_METADATA) {
        player.currentTime = seconds;
      } else {
        player.addEventListener("loadedmetadata", () => (player.currentTime = seconds), {
          once: true,
        });
      }
    }
    this.playhead.set(seconds);
  }

  onTimeUpdate(): void {
    const el = this.audible();
    if (el) this.playhead.set(el.currentTime);
  }

  onEnded(): void {
    this.playing.set(false);
  }

  private settle(side: Side): void {
    const rest = new Set(this.waiting());
    rest.delete(side);
    this.waiting.set(rest);
  }

  private both(): HTMLAudioElement[] {
    return [this.playerA()?.nativeElement, this.playerB()?.nativeElement].filter(
      (el): el is HTMLAudioElement => !!el,
    );
  }

  private audible(): HTMLAudioElement | undefined {
    return (this.side() === "a" ? this.playerA() : this.playerB())?.nativeElement;
  }

  /** One player at full volume, the other silent but still running. */
  private applySide(): void {
    const [a, b] = [this.playerA()?.nativeElement, this.playerB()?.nativeElement];
    if (a) a.muted = this.side() !== "a";
    if (b) b.muted = this.side() !== "b";
  }
}
