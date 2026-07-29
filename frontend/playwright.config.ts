import { defineConfig, devices } from "@playwright/test";

/**
 * Layout harness (the fleet's shared phone-width checks): render the production
 * build in a real browser at true device geometry and assert about the painted
 * pixels — text overlap, horizontal overflow, occluded controls. It runs against
 * the built bundle via e2e/serve.mjs.
 *
 * Tests live in e2e/ (outside src/), so the unit runner ignores them.
 */
// Unique across the fleet. Every app's harness sets `reuseExistingServer: true`,
// so two apps sharing a port silently attach to each OTHER's server: the second
// run serves the first app's bundle, every locator times out, and when the first
// run ends its server dies and the rest fail ERR_CONNECTION_REFUSED. This was
// 4293, which is recall's, and it made recall's e2e fail 8/8 whenever the two ran
// at once — invisible in isolation, where both pass in ~3s.
const PORT = 4292;

export default defineConfig({
  testDir: "./e2e",
  reporter: [["list"]],
  timeout: 90_000,
  use: {
    baseURL: `http://localhost:${PORT}`,
    screenshot: "only-on-failure",
  },
  // Pixel 7 preset (412 CSS px, mobile UA, touch). The viewport MUST live in the
  // project's `use` — a device spread carries its own viewport and project-level
  // `use` overrides global — and the first test guards against it silently
  // reverting to a desktop width.
  projects: [{ name: "chromium", use: { ...devices["Pixel 7"], deviceScaleFactor: 1 } }],
  webServer: {
    command: `node e2e/serve.mjs ${PORT}`,
    url: `http://localhost:${PORT}/`,
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
