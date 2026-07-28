import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatCardModule } from "@angular/material/card";
import { MatToolbarModule } from "@angular/material/toolbar";
import { RouterLink, RouterLinkActive, RouterOutlet } from "@angular/router";

import { AuthState } from "./auth";

@Component({
  selector: "app-root",
  templateUrl: "./app.html",
  styleUrl: "./app.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    MatButtonModule,
    MatCardModule,
    MatToolbarModule,
    RouterLink,
    RouterLinkActive,
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
}
