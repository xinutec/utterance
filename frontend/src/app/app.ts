import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { BreakpointObserver, Breakpoints } from "@angular/cdk/layout";
import { MatCardModule } from "@angular/material/card";
import { MatIconModule } from "@angular/material/icon";
import { MatMenuModule } from "@angular/material/menu";
import { MatToolbarModule } from "@angular/material/toolbar";
import { NavigationEnd, Router, RouterLink, RouterOutlet } from "@angular/router";
import { toSignal } from "@angular/core/rxjs-interop";
import { filter, map } from "rxjs";

import { AuthState } from "./auth";
import { Telemetry } from "./telemetry";

@Component({
  selector: "app-root",
  templateUrl: "./app.html",
  styleUrl: "./app.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    MatButtonModule,
    MatCardModule,
    MatIconModule,
    MatMenuModule,
    MatToolbarModule,
    RouterLink,
    RouterOutlet,
  ],
})
export class App {
  /**
   * The client activity trace, started here because this is the one component
   * guaranteed to exist for the app's whole life.
   *
   * Wired in the shell rather than per page for the reason the service exists:
   * a trace that each screen has to remember to join is a trace with holes in
   * exactly the screens nobody thought about.
   */
  private readonly telemetry = inject(Telemetry);

  /**
   * Read by the template to decide whether there is an app to show.
   *
   * Nothing sets this on startup — it is raised by the first request the
   * backend refuses. On a deployment with no sign-in configured it stays false
   * forever and the wall is not merely hidden but never rendered.
   */
  readonly auth = inject(AuthState);

  /**
   * Every page, described once.
   *
   * The bar renders this list twice — as buttons on a wide screen and as menu
   * items on a narrow one — so writing the destinations out in the template
   * would mean maintaining each of them in two places, and the copy that drifts
   * is always the one nobody has open.
   */
  readonly pages = [
    // `exact` on the studio alone: every route is a prefix of "/", so without it
    // the studio reads as current on every page.
    // In the order somebody does them: nothing works before a voice exists.
    { path: "/calibrate", label: "Calibrate", exact: false },
    { path: "/", label: "Studio", exact: true },
    { path: "/compare", label: "Compare", exact: false },
  ] as const;

  private readonly breakpoints = inject(BreakpointObserver);

  /**
   * Whether there is only room for one button.
   *
   * The same `Breakpoints.Handset` recall uses, so the two apps collapse at the
   * same width rather than at two hand-picked ones. On a wide screen the
   * destinations are worth their space; on a phone they are not, and there is
   * nothing else for the menu to hold — which is why the button appears only
   * here, where recall keeps one at every width for an overflow set utterance
   * does not have.
   */
  readonly handset = toSignal(
    this.breakpoints.observe(Breakpoints.Handset).pipe(map((state) => state.matches)),
    { initialValue: false },
  );

  private readonly router = inject(Router);

  /**
   * Where the app currently is.
   *
   * Held here rather than read from `routerLinkActive`, so that which item is
   * current and which is announced to a screen reader are one fact instead of a
   * class and an attribute that have to agree.
   *
   * **Not because the directive fails inside a menu.** It was replaced on that
   * belief and the belief was wrong: measured against a `mat-menu`, both the
   * bare and class-carrying forms report `isActive` correctly once the menu
   * opens. What went wrong in the original markup was never established. This
   * version is kept because it works and says what it means, not because the
   * directive could not.
   */
  private readonly url = toSignal(
    this.router.events.pipe(
      filter((event) => event instanceof NavigationEnd),
      map(() => this.router.url),
    ),
    { initialValue: this.router.url },
  );

  /**
   * Whether `path` is the page being shown.
   *
   * The studio matches exactly and everything else by prefix, because every
   * route is a prefix of `"/"` — without that, the studio would read as current
   * on every page. Query strings are ignored: `/compare?take=…` is still the
   * compare page.
   */
  isCurrent(page: { path: string; exact: boolean }): boolean {
    const here = this.url().replace(/\?.*$/, "");
    return page.exact ? here === page.path : here.startsWith(page.path);
  }

  constructor() {
    // After the field initialisers, so the router this subscribes to exists.
    // Idempotent, so a shell recreated in a test does not stack listeners.
    this.telemetry.init();
  }
}
