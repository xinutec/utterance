import { Injectable, inject } from "@angular/core";
import { MatSnackBar } from "@angular/material/snack-bar";

/**
 * The app's snackbar grammar in one place, the fleet's shape (life's
 * `shared/feedback.ts`), so a feature never opens `MatSnackBar` itself.
 */
@Injectable({ providedIn: "root" })
export class Feedback {
  private readonly snack = inject(MatSnackBar);

  /**
   * Offer Undo for a removal that has not happened yet: `onUndo` reverses the
   * hiding, `onCommit` does the removal once the bar closes without a tap.
   * Deferred rather than reversed, so a page closed mid-window keeps the data.
   */
  undo(message: string, onUndo: () => void, onCommit: () => void): void {
    const ref = this.snack.open(message, "Undo", { duration: 6000 });
    let undone = false;
    ref.onAction().subscribe(() => {
      undone = true;
      onUndo();
    });
    ref.afterDismissed().subscribe(() => {
      if (!undone) onCommit();
    });
  }
}
