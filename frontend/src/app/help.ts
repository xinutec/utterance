import { ChangeDetectionStrategy, Component } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";
import { MatMenuModule } from "@angular/material/menu";

/**
 * A question mark that opens what would otherwise be a paragraph. A page says
 * the short true thing and hides the rest behind this: prose read once is in the
 * way for ever after, and goes stale silently. Opened by click, since a phone
 * has no hover.
 *
 * ```html
 * <app-help>Longer explanation, in as many words as it needs.</app-help>
 * ```
 */
@Component({
  selector: "app-help",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MatButtonModule, MatIconModule, MatMenuModule],
  templateUrl: "./help.html",
  styleUrl: "./help.scss",
})
export class Help {}
