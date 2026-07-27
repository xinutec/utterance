import { ChangeDetectionStrategy, Component } from "@angular/core";
import { MatToolbarModule } from "@angular/material/toolbar";
import { RouterOutlet } from "@angular/router";

@Component({
  selector: "app-root",
  templateUrl: "./app.html",
  styleUrl: "./app.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MatToolbarModule, RouterOutlet],
})
export class App {}
