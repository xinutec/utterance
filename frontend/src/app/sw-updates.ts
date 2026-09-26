import { Injectable, inject } from '@angular/core';
import { SwUpdate, VersionReadyEvent } from '@angular/service-worker';
import {
  type PagePort,
  type ServiceWorkerPort,
  SwUpdates as SwUpdatePolicy,
  type UpdateOutcome,
} from '@xinutec/ui-harness/sw-updates';
import { filter } from 'rxjs';

export type { UpdateOutcome };

/**
 * Marks that we have already auto-reloaded out of an unrecoverable service
 * worker state. Session-scoped so it survives that very reload. Keep the key
 * stable: a tab mid-recovery across an upgrade must still see its mark.
 */
const RECOVERY_KEY = 'utterance.sw-recovery-attempted';

/**
 * Self-update — the Angular adapter for `@xinutec/ui-harness/sw-updates`, whose
 * policy is tested there against a fake. ⚠ ngsw alone caches a build that never
 * learns a newer one exists, serving yesterday's app while looking fine.
 */
@Injectable({ providedIn: 'root' })
export class SwUpdates {
  private readonly sw = inject(SwUpdate);

  private readonly serviceWorker: ServiceWorkerPort = ((sw: SwUpdate) => ({
    // A local, not `this`: an object-literal getter would not capture `this`,
    // and a copied boolean would freeze `isEnabled`.
    get isEnabled(): boolean {
      return sw.isEnabled;
    },
    onVersionReady: (handler: () => void): void => {
      sw.versionUpdates
        .pipe(filter((event): event is VersionReadyEvent => event.type === 'VERSION_READY'))
        .subscribe(() => handler());
    },
    onUnrecoverable: (handler: () => void): void => {
      // A broken cache whose files the server no longer has (after a
      // roll-forward): only a fresh load recovers.
      sw.unrecoverable.subscribe(() => handler());
    },
    checkForUpdate: () => sw.checkForUpdate(),
    activateUpdate: () => sw.activateUpdate(),
  }))(this.sw);

  private readonly page: PagePort = {
    get hidden(): boolean {
      return document.visibilityState === 'hidden';
    },
    onVisibilityChange: (handler: () => void): void => {
      document.addEventListener('visibilitychange', handler);
    },
    recoveryAttempted: () => sessionStorage.getItem(RECOVERY_KEY) !== null,
    markRecoveryAttempted: () => sessionStorage.setItem(RECOVERY_KEY, '1'),
    // Through the method below, so a test can assert the reload.
    reload: () => this.reload(),
    now: () => Date.now(),
  };

  private readonly policy = new SwUpdatePolicy(this.serviceWorker, this.page);

  start(): void {
    this.policy.start();
  }

  /** Manual "Check for updates". Never rejects: failure comes back `'failed'`. */
  checkNow(): Promise<UpdateOutcome> {
    return this.policy.checkNow();
  }

  /** The one place the page is thrown away, a method so tests can assert it. */
  reload(): void {
    document.location.reload();
  }
}
