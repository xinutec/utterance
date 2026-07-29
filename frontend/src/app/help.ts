import { ChangeDetectionStrategy, Component } from "@angular/core";
import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";
import { MatMenuModule } from "@angular/material/menu";

/**
 * A question mark that opens what would otherwise be a paragraph.
 *
 * **Why explanation is a control rather than prose.** This app is used by one
 * singer who will meet each page a handful of times and then know it. A screen
 * that explains itself in sentences reads well once and is in the way for ever
 * after — and the sentences are always the first thing to go stale, because
 * nothing fails when they stop being true.
 *
 * So a page says the short true thing, and anything longer goes behind this. The
 * cost of asking is one click; the cost of not asking is nothing at all.
 *
 * Opened by click rather than hover: a hover tooltip does not exist on a phone,
 * and the page it is used on is used on a phone.
 *
 * ```html
 * <app-help>
 *   Longer explanation, in as many words as it actually needs.
 * </app-help>
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
