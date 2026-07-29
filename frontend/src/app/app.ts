import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatCardModule } from "@angular/material/card";
import { MatIconModule } from "@angular/material/icon";
import { MatMenuModule } from "@angular/material/menu";
import { MatToolbarModule } from "@angular/material/toolbar";
import { NavigationEnd, Router, RouterLink, RouterOutlet } from "@angular/router";
import { toSignal } from "@angular/core/rxjs-interop";
import { filter, map } from "rxjs";

import { AuthState } from "./auth";

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
   * Read by the template to decide whether there is an app to show.
   *
   * Nothing sets this on startup — it is raised by the first request the
   * backend refuses. On a deployment with no sign-in configured it stays false
   * forever and the wall is not merely hidden but never rendered.
   */
  readonly auth = inject(AuthState);

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
  isCurrent(path: string): boolean {
    const here = this.url().split("?")[0];
    return path === "/" ? here === "/" : here.startsWith(path);
  }
}
