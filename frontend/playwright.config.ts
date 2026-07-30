import { defineConfig, devices } from "@playwright/test";
import { phoneConfig } from "@xinutec/ui-harness/config";
import harness from "./e2e/harness.mjs";

/**
 * Layout harness (the fleet's shared phone-width checks): render the production
 * build in a real browser at true device geometry and assert about the painted
 * pixels — text overlap, horizontal overflow, occluded controls.
 *
 * Everything shared — the Pixel geometry, the port, the static server that
 * serves the built bundle — comes from @xinutec/ui-harness. What this app says
 * about itself is in e2e/harness.mjs.
 *
 * Tests live in e2e/ (outside src/), so the unit runner ignores them.
 */
export default defineConfig(phoneConfig(harness, devices));
