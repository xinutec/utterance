import { defineConfig, devices } from "@playwright/test";
import { phoneConfig } from "@xinutec/ui-harness/config";
import harness from "./e2e/harness.mjs";

/**
 * The layout harness: the production build in a real browser at phone
 * geometry, asserting on painted pixels. Shared setup comes from
 * @xinutec/ui-harness; this app's part is e2e/harness.mjs.
 */
export default defineConfig(phoneConfig(harness, devices));
