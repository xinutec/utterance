import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatCardModule } from "@angular/material/card";
import { MatIconModule } from "@angular/material/icon";
import { MatMenuModule } from "@angular/material/menu";
import { RouterLink, RouterOutlet } from "@angular/router";
import { Scaffold } from "@xinutec/ui-scaffold";

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
    RouterLink,
    RouterOutlet,
    Scaffold,
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

  constructor() {
    // After the field initialisers, so the router exists; idempotent, so a
    // recreated shell does not stack listeners. The same for service-worker
    // updates.
    this.telemetry.init();
    this.swUpdates.start();
  }
}
