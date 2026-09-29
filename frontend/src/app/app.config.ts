import { provideHttpClient, withFetch, withInterceptors } from "@angular/common/http";
import { ErrorHandler,
  ApplicationConfig,
  isDevMode,
  provideBrowserGlobalErrorListeners,
  provideZonelessChangeDetection,
} from "@angular/core";
import { provideRouter, withComponentInputBinding } from "@angular/router";

import { provideServiceWorker } from "@angular/service-worker";

import { routes } from "./app.routes";
import { authInterceptor } from "./auth";
import { TelemetryErrorHandler, failedRequestInterceptor } from './error-reporting';

export const appConfig: ApplicationConfig = {
  providers: [
    { provide: ErrorHandler, useClass: TelemetryErrorHandler },
    provideZonelessChangeDetection(),
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withComponentInputBinding()),
    provideHttpClient(withFetch(), withInterceptors([authInterceptor, failedRequestInterceptor])),
    provideServiceWorker("ngsw-worker.js", {
      enabled: !isDevMode(),
      registrationStrategy: "registerWhenStable:30000",
    }),
  ],
};
