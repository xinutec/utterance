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
import { SwUpdates } from "./sw-updates";
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
   * The client activity trace, started in the shell — the one component alive
   * for the app's whole life — so no screen has to remember to join it.
   */
  private readonly telemetry = inject(Telemetry);
  private readonly swUpdates = inject(SwUpdates);

  /**
   * Whether the sign-in wall shows. Raised by the first request the backend
   * refuses; with no sign-in configured it never is.
   */
  readonly auth = inject(AuthState);

  /** Every page, described once for both the button bar and the menu. */
  readonly pages = [
    // In the order somebody does them: nothing works before a voice exists.
    { path: "/calibrate", label: "Calibrate", exact: false },
    { path: "/", label: "Studio", exact: true },
    { path: "/compare", label: "Compare", exact: false },
  ] as const;

  private readonly breakpoints = inject(BreakpointObserver);

  /**
   * Whether there is only room for one button — recall's `Breakpoints.Handset`,
   * so both apps collapse at the same width.
   */
  readonly handset = toSignal(
    this.breakpoints.observe(Breakpoints.Handset).pipe(map((state) => state.matches)),
    { initialValue: false },
  );

  private readonly router = inject(Router);

  /**
   * Where the app currently is — one fact for both the highlight and
   * `aria-current`, rather than a class and an attribute that must agree.
   */
  private readonly url = toSignal(
    this.router.events.pipe(
      filter((event) => event instanceof NavigationEnd),
      map(() => this.router.url),
    ),
    { initialValue: this.router.url },
  );

  /**
   * Whether `path` is the page being shown: the studio exactly (every route is a
   * prefix of `/`), the rest by prefix, ignoring query strings.
   */
  isCurrent(page: { path: string; exact: boolean }): boolean {
    const here = this.url().replace(/\?.*$/, "");
    return page.exact ? here === page.path : here.startsWith(page.path);
  }

  constructor() {
    // After the field initialisers, so the router exists; idempotent, so a
    // recreated shell does not stack listeners. The same for service-worker
    // updates.
    this.telemetry.init();
    this.swUpdates.start();
  }
}
