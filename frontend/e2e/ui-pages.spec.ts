import { expect, test, type Page } from "@playwright/test";
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
 * studio is a dense page — a take list, a stats line, a four-panel chart and a vowel-space plot —
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
  peak: 0.71,
  clipped: false,
};

/** A voiceprint with enough frames that the chart draws real curves. */
function voiceprint(): unknown {
  const count = 400;
  const frames = Array.from({ length: count }, (_, i) => i);
  return {
    schemaVersion: 4,
    source: { sampleRateHz: 48_000, channels: 1, durationS: 28.4, peak: 0.71, clippedFraction: 0 },
    frame: { analysisRateHz: 16_000, hopS: 0.01, count },
    pitch: {
      // A contour with unvoiced gaps, so the multi-stroke path is exercised.
      hz: frames.map((i) => (i % 50 < 30 ? 120 + 40 * Math.sin(i / 12) : null)),
      aperiodicity: frames.map((i) => (i % 50 < 30 ? 0.08 : 0.9)),
    },
    formants: {
      f1: frames.map((i) => (i % 50 < 30 ? 300 + 300 * Math.sin(i / 30) : null)),
      f2: frames.map((i) => (i % 50 < 30 ? 1400 + 700 * Math.cos(i / 30) : null)),
      f3: frames.map((i) => (i % 50 < 30 ? 2700 : null)),
    },
    rmsDb: frames.map((i) => (i % 50 < 30 ? -18 + 6 * Math.sin(i / 7) : -70)),
    events: {
      flux: frames.map((i) => (i % 50 === 0 ? 1 : Math.abs(Math.sin(i / 5)) * 0.2)),
      onsetFrames: frames.filter((i) => i % 50 === 0),
      onsetTimesS: frames.filter((i) => i % 50 === 0).map((i) => i * 0.01),
    },
    partials: {
      framesUsed: 240,
      f0Hz: 119.3,
      partials: Array.from({ length: 12 }, (_, k) => ({
        number: k + 1,
        ratio: k + 1,
        amplitude: 1 / (k + 1),
        presence: 1,
      })),
    },
  };
}

/** A speaker's derived scale, roughly what a real harmonic voice produces. */
const VOICE = {
  tonicHz: 119.7,
  degrees: [
    { cents: 0, ratio: 1, depth: 0 },
    { cents: 316, ratio: 1.2, depth: 0.097 },
    { cents: 386, ratio: 1.25, depth: 0.053 },
    { cents: 582, ratio: 1.4, depth: 0.041 },
    { cents: 702, ratio: 1.5, depth: 0.138 },
    { cents: 884, ratio: 1.666, depth: 0.155 },
    { cents: 1200, ratio: 2, depth: 0 },
  ],
  timbre: Array.from({ length: 24 }, (_, k) => 1 / (k + 1)),
  calibrationId: "0123456789abcdef",
  calibrationLabel: "steady-ah",
  takes: 7,
};

/** Catch-all first, then the specific routes. */
async function mockApi(page: Page): Promise<void> {
  await page.route("**/api/**", (r) =>
    r.request().method() === "GET" ? r.fulfill({ json: [] }) : r.fulfill({ status: 204, body: "" }),
  );
  await page.route("**/api/recordings", (r) => r.fulfill({ json: [META] }));
  await page.route("**/api/recordings/0123456789abcdef", (r) =>
    r.fulfill({ json: { meta: META, voiceprint: voiceprint() } }),
  );
  await page.route("**/api/voice", (r) => r.fulfill({ json: VOICE }));
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
  // Two canvases now — the time-series chart and the vowel space. Wait for
  // both, so the layout assertions run against the fully painted page.
  await page.locator("app-voiceprint-chart canvas").waitFor();
  await page.locator("app-vowel-space canvas").waitFor();

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

test("calibration — the guided steps lay out cleanly @ phone", async ({ page }, testInfo) => {
  // This page is read while someone is standing at a microphone, so its failure
  // mode is worse than the studio's: an instruction line colliding with the
  // record button is the difference between a usable take and a wasted one.
  await mockApi(page);
  await page.goto("/calibrate");
  await page.getByText('Hold "ah" for about ten seconds, as steady as you can.').waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("calibration — the longest step still fits @ phone", async ({ page }, testInfo) => {
  // The speech step carries the longest instruction lines in the app, so it is
  // the one that overflows first if the detail list ever stops wrapping.
  await mockApi(page);
  await page.goto("/calibrate");
  await page.getByRole("button", { name: "Talk normally" }).click();
  await page.getByText("Talk about anything for about a minute.").waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("studio — the derived scale lays out cleanly @ phone", async ({ page }, testInfo) => {
  // The densest row in the app: four numeric columns and a bar, one line per
  // scale degree. It is the first thing to collide when the viewport narrows.
  await mockApi(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Render as music" }).click();
  await page.getByText("The scale this voice implies").waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

/**
 * Canvas drawing takes colour strings, and an unparseable one is ignored in
 * silence — `fillStyle` simply keeps its previous value, which starts out black.
 * Material's system tokens compute to `light-dark(#1a1b1f, #e3e2e6)`, a CSS
 * function no canvas can parse, so passing one straight through painted black
 * text on a dark background with nothing anywhere reporting a problem.
 *
 * Nothing else in this suite can see that: the layout checks measure geometry,
 * the unit tests never rasterise, and the page is perfectly valid. So this reads
 * the pixels.
 */
for (const scheme of ["light", "dark"] as const) {
  test(`canvases stay legible in ${scheme} mode`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await mockApi(page);
    await page.goto("/");
    await page.locator("app-voiceprint-chart canvas").waitFor();
    await page.locator("app-vowel-space canvas").waitFor();

    for (const selector of ["app-voiceprint-chart canvas", "app-vowel-space canvas"]) {
      const contrast = await page.locator(selector).evaluate((canvas: HTMLCanvasElement) => {
        const relativeLuminance = (r: number, g: number, b: number): number => {
          const channel = (v: number): number => {
            const s = v / 255;
            return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
          };
          return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
        };

        const background = getComputedStyle(document.body).backgroundColor;
        const [br, bg, bb] = background.match(/\d+/g)!.map(Number);
        const backgroundLuminance = relativeLuminance(br, bg, bb);

        const ctx = canvas.getContext("2d")!;
        const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);

        // Solidly painted pixels only — antialiased glyph edges blend toward the
        // background by design and would drag the measurement down.
        const ratios: number[] = [];
        for (let i = 0; i < data.length; i += 4) {
          if (data[i + 3] < 200) continue;
          const l = relativeLuminance(data[i], data[i + 1], data[i + 2]);
          const [hi, lo] = l > backgroundLuminance ? [l, backgroundLuminance] : [backgroundLuminance, l];
          ratios.push((hi + 0.05) / (lo + 0.05));
        }
        if (ratios.length === 0) return { painted: 0, best: 0 };
        ratios.sort((a, b) => a - b);
        return { painted: ratios.length, best: ratios[Math.floor(ratios.length * 0.9)] };
      });

      expect(contrast.painted, `${selector} painted nothing at all`).toBeGreaterThan(200);
      expect(
        contrast.best,
        `${selector} in ${scheme} mode: brightest marks reach only ${contrast.best.toFixed(1)}:1 against the page`,
      ).toBeGreaterThan(3);
    }
  });
}
