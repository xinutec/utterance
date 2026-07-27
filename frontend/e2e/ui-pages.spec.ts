import { test, type Page } from "@playwright/test";
// The fleet-shared layout harness, consumed as the published @xinutec/ui-harness
// package (source repo ~/Code/ui-harness). Ships compiled JS, loads from
// node_modules.
import {
  expectNoTextOverlaps,
  expectNoHorizontalOverflow,
  expectNoOccludedControls,
  expectViewportIsPhone,
} from "@xinutec/ui-harness";

/**
 * Layout-measurement checks against the built bundle with the API mocked. The
 * studio is a dense page — a take list, a stats line and a three-panel chart —
 * and its failure modes (a stats line colliding with the delete button, a canvas
 * forcing the body wider than the screen) read fine in source and only show in a
 * real browser at a real width.
 */

/** One stored take, enough for the list and the detail pane to populate. */
const META = {
  id: "0123456789abcdef",
  label: "brother — take 1",
  createdAtMs: 1_700_000_000_000,
  durationS: 28.4,
  sampleRateHz: 48_000,
  voicedFraction: 0.62,
  onsetCount: 74,
};

/** A voiceprint with enough frames that the chart draws real curves. */
function voiceprint(): unknown {
  const count = 400;
  const frames = Array.from({ length: count }, (_, i) => i);
  return {
    schemaVersion: 1,
    source: { sampleRateHz: 48_000, channels: 1, durationS: 28.4 },
    frame: { analysisRateHz: 16_000, hopS: 0.01, count },
    pitch: {
      // A contour with unvoiced gaps, so the multi-stroke path is exercised.
      hz: frames.map((i) => (i % 50 < 30 ? 120 + 40 * Math.sin(i / 12) : null)),
      aperiodicity: frames.map((i) => (i % 50 < 30 ? 0.08 : 0.9)),
    },
    rmsDb: frames.map((i) => (i % 50 < 30 ? -18 + 6 * Math.sin(i / 7) : -70)),
    events: {
      flux: frames.map((i) => (i % 50 === 0 ? 1 : Math.abs(Math.sin(i / 5)) * 0.2)),
      onsetFrames: frames.filter((i) => i % 50 === 0),
      onsetTimesS: frames.filter((i) => i % 50 === 0).map((i) => i * 0.01),
    },
  };
}

/** Catch-all first, then the specific routes. */
async function mockApi(page: Page): Promise<void> {
  await page.route("**/api/**", (r) =>
    r.request().method() === "GET" ? r.fulfill({ json: [] }) : r.fulfill({ status: 204, body: "" }),
  );
  await page.route("**/api/recordings", (r) => r.fulfill({ json: [META] }));
  await page.route("**/api/recordings/0123456789abcdef", (r) =>
    r.fulfill({ json: { meta: META, voiceprint: voiceprint() } }),
  );
}

test("the suite really runs at phone geometry", async ({ page }) => {
  await mockApi(page);
  await page.goto("/");
  await expectViewportIsPhone(page);
});

test("studio — take list and voiceprint lay out cleanly @ phone", async ({ page }, testInfo) => {
  await mockApi(page);
  await page.goto("/");
  await page.getByText("brother — take 1").first().waitFor();
  await page.locator("canvas").waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("studio — empty state lays out cleanly @ phone", async ({ page }, testInfo) => {
  // The first thing anyone sees, and the state where the record button has to
  // be reachable without scrolling past anything.
  await page.route("**/api/**", (r) => r.fulfill({ json: [] }));
  await page.goto("/");
  await page.getByText("Nothing recorded yet.").waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});
