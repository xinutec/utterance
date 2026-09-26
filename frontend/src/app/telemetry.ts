import { DOCUMENT, Injectable, inject } from "@angular/core";
import { NavigationEnd, Router } from "@angular/router";
import { TelemetryCore } from "@xinutec/ui-harness/telemetry";
import { filter } from "rxjs";

/**
 * The Angular binding for the fleet's activity trace; the queue, flushing and
 * transport live in `@xinutec/ui-harness/telemetry`. An `@Injectable` cannot
 * ship from that package — it is built by plain `tsc`, and a production build
 * fails with `JIT compiler unavailable` — so the capture seams and DI are here.
 * Wired once from the app shell, so no screen has to opt in.
 */
@Injectable({ providedIn: "root" })
export class Telemetry {
  private readonly router = inject(Router);
  private readonly doc = inject(DOCUMENT);
  private readonly core = new TelemetryCore(this.doc);

  /** Wire the two capture points. Called once from the app shell; idempotent. */
  init(): void {
    if (this.core.started) return;

    this.router.events
      .pipe(filter((e): e is NavigationEnd => e instanceof NavigationEnd))
      .subscribe((e) => this.core.record("nav", e.urlAfterRedirects, null));

    // Capture phase, so the tap is seen even where a handler stops propagation.
    this.doc.addEventListener("click", (ev) => this.core.recordTap(ev.target, this.router.url), {
      capture: true,
    });

    this.core.start();
  }
}
