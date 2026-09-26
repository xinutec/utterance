import { expect, test, type Page } from "@playwright/test";
// The fleet-shared layout harness (@xinutec/ui-harness), from node_modules.
import {
  expectNoTextOverlaps,
  expectNoHorizontalOverflow,
  expectNoStarvedText,
  expectNoOccludedControls,
  expectViewportIsPhone,
} from "@xinutec/ui-harness";

/**
 * Layout checks against the built bundle with the API mocked: collisions and
 * overflow read fine in source and only show in a real browser at phone width.
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
  role: "calibration",
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
    texture: {
      // Tonal in the bursts, noisy between: the shape a consonant makes.
      centroidHz: frames.map((i) => (i % 50 < 30 ? 700 : 5200)),
      flatness: frames.map((i) => (i % 50 < 30 ? 0.02 : 0.8)),
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
  palette: [
    Array.from({ length: 24 }, (_, k) => 1 / (k + 1)),
    Array.from({ length: 24 }, (_, k) => 0.2 + 0.03 * k),
  ],
  detuneCents: 3.4,
  calibrationId: "0123456789abcdef",
  calibrationLabel: "steady-ah",
  takes: 7,
  // Present and null, not absent: a missing field makes its assertions vacuous.
  refusal: null,
};

/** What the backend says when the chosen mapping has no answer for this scale. */
const REFUSAL =
  "Lattice cannot be played in this scale: this voice's scale has one interval " +
  "(702¢) besides the tonic and the octave, and a lattice is spanned by two " +
  "intervals pointing different ways. Lowering the scale density keeps more of them.";

/**
 * The mapping's knobs, copied from `utterance_mapping::params::KNOBS` — this
 * suite is about layout, and `tests/api.rs` checks the real ranges. `mappings`
 * matters: the controls hide knobs the playing mapping ignores, so without it
 * no sliders render and every assertion passes over an empty page.
 */
const CONTROLS = {
  knobs: [
    { name: "bind", label: "Bind to the voice", min: 0, max: 1, step: 0.05, default: 1, mappings: [], about: "At 1 the notes are exactly where this voice's spectrum puts them. At 0 they snap to the twelve everyone else uses.", primary: true },
    { name: "density", label: "Scale density", min: 0.0005, max: 0.5, step: 0.002, default: 0.02, mappings: [], about: "How firm a note has to be to count. Low gives a crowded microtonal set, high gives a handful of very stable intervals.", primary: true },
    { name: "voices", label: "Voices", min: 1, max: 12, step: 1, default: 5, mappings: ["field", "tonnetz"], about: "How many tones sound at once.", primary: true },
    { name: "spacing", label: "Spacing", min: 1, max: 6, step: 1, default: 2, mappings: ["field", "tonnetz"], about: "How far apart the voices sit. 1 is a cluster, higher is an open chord.", primary: true },
    { name: "drift", label: "Follow the pitch", min: 0, max: 2, step: 0.05, default: 0.25, mappings: ["field", "tonnetz"], about: "How far the music transposes with the speaker's pitch. At 0 it sits still; near 1 it reads as a parallel melody.", primary: false },
    { name: "reach", label: "Follow the vowel", min: 0, max: 3, step: 0.05, default: 1, mappings: ["field", "tonnetz"], about: "How far the vowel moves the harmony. This is the articulation showing up as harmony.", primary: false },
    { name: "hold", label: "Hold the harmony", min: 0, max: 1, step: 0.05, default: 0.35, mappings: ["tonnetz"], about: "How far the mouth must move past a boundary before the chord changes. At 0 the harmony follows every wobble.", primary: true },
    { name: "consonants", label: "Consonants", min: 0, max: 2, step: 0.05, default: 1, mappings: [], about: "How loud the unpitched material is against the tones. At 0 they are silent.", primary: false },
  ],
  mappings: [
    { name: "field", label: "Field", makes: "texture", about: "Every frame sounds." },
    { name: "tonnetz", label: "Lattice", makes: "texture", about: "The vowel walks a harmonic lattice." },
    { name: "notes", label: "Notes", makes: "events", about: "Discrete events at onsets." },
  ],
};
/**
 * A score, as the compare page charts it: long enough to fill the canvas, with
 * enough degrees to wrap the scale caption on a phone.
 */
function score(offset: number) {
  const points = 600;
  const at = (i: number) => i / points;
  return {
    durationS: 46.4,
    stepS: 46.4 / points,
    colour: Array.from({ length: points }, (_, i) => 0.4 + 0.3 * Math.sin(at(i) * 12 + offset)),
    breath: Array.from({ length: points }, (_, i) => 0.05 + 0.03 * Math.cos(at(i) * 20 + offset)),
    level: Array.from({ length: points }, (_, i) => 0.5 + 0.4 * Math.sin(at(i) * 7 + offset)),
    voices: [
      Array.from({ length: points }, (_, i) => 120 + 20 * Math.sin(at(i) * 5 + offset)),
      Array.from({ length: points }, (_, i) => 480 + 60 * Math.sin(at(i) * 5 + offset)),
    ],
    gains: [Array.from({ length: points }, () => 0.6), Array.from({ length: points }, () => 0.3)],
    degrees: [0, 316, 386, 582, 702, 813, 884, 1200],
    consonants: [1.2, 4.8, 9.1],
    events: [],
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
  // Trailing wildcard: the summary carries settings in its query, and a glob
  // without one would silently fall through to the catch-all.
  await page.route("**/api/voice*", (r) => r.fulfill({ json: VOICE }));
  await page.route("**/api/controls", (r) => r.fulfill({ json: CONTROLS }));
  // The two sides told apart by the query, so the charts differ.
  await page.route("**/score*", (r) =>
    r.fulfill({ json: score(r.request().url().includes("bind=0") ? 2 : 0) }),
  );
}

/**
 * A strip beside the sliders a thumb can land on without turning a knob. A
 * Material slider takes its value where a pointer goes down, before the browser
 * decides it was a scroll, so a full-width column leaves nowhere to scroll from.
 * Every slider is checked: the one that reaches the edge is the one a spot
 * check misses.
 */
async function expectSomewhereToScrollFrom(page: Page) {
  const gutters = await page.locator("app-mapping-controls .knob").evaluateAll((knobs) =>
    knobs.map((knob) => {
      const slider = knob.querySelector("mat-slider");
      if (!slider) return null;
      return Math.round(knob.getBoundingClientRect().right - slider.getBoundingClientRect().right);
    }),
  );

  expect(gutters.length, "no knobs to check").toBeGreaterThan(0);
  for (const gutter of gutters) {
    // 44 px of touch target, less rounding and the slider's end padding.
    expect(gutter, `a slider reaches the edge, leaving ${gutter}px to scroll from`)
      .toBeGreaterThanOrEqual(40);
  }
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
  // Wait for both canvases, so the page is fully painted.
  await page.locator("app-voiceprint-chart canvas").waitFor();
  await page.locator("app-vowel-space canvas").waitFor();
  // The knobs are the densest thing on the page.
  await page.locator("app-mapping-controls mat-slider").last().waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoStarvedText(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
  await expectSomewhereToScrollFrom(page);
});

test("studio — empty state lays out cleanly @ phone", async ({ page }, testInfo) => {
  // The record button must be reachable without scrolling.
  await page.route("**/api/**", (r) => r.fulfill({ json: [] }));
  await page.goto("/");
  await page.getByText("Nothing recorded yet.").waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoStarvedText(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("calibration — the guided steps lay out cleanly @ phone", async ({ page }, testInfo) => {
  // Read while standing at a microphone: an instruction colliding with the
  // record button wastes a take.
  await mockApi(page);
  await page.goto("/calibrate");
  await page.getByText('Hold "ah" for about ten seconds, as steady as you can.').waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoStarvedText(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("calibration — the longest step still fits @ phone", async ({ page }, testInfo) => {
  // The speech step has the longest instructions, so it overflows first.
  await mockApi(page);
  await page.goto("/calibrate");
  await page.getByRole("button", { name: "Talk normally" }).click();
  await page.getByText("Talk about anything for about a minute.").waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoStarvedText(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("studio — the derived scale lays out cleanly @ phone", async ({ page }, testInfo) => {
  // The densest row in the app: four numeric columns and a bar per degree.
  await mockApi(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Render as music" }).click();
  await page.getByText("The scale this voice implies").waitFor();

  // The click scrolled content under the opaque sticky toolbar, which the
  // overlap check would flag; return to the top to measure layout, not scroll.
  await page.evaluate(() => {
    window.scrollTo(0, 0);
  });

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoStarvedText(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("studio — a scale that carries no lattice says so @ phone", async ({ page }, testInfo) => {
  // Without this message the player would play consonants over silence; it is
  // also the prose likeliest to overflow a phone.
  await mockApi(page);
  await page.route("**/api/voice*", (r) => r.fulfill({ json: { ...VOICE, refusal: REFUSAL } }));
  await page.goto("/");
  await page.getByRole("button", { name: "Render as music" }).click();
  await page.getByRole("alert").filter({ hasText: "Lattice cannot be played" }).waitFor();

  // No player for the derived music: a refused render shows a broken control.
  // Scoped, since the recording's own player is fine.
  await expect(page.locator("app-derived-music audio.player")).toHaveCount(0);

  await page.evaluate(() => {
    window.scrollTo(0, 0);
  });
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoStarvedText(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("the sign-in wall lays out cleanly @ phone", async ({ page }, testInfo) => {
  // The first thing anyone sees off the LAN, and a centred card no other page
  // has.
  await mockApi(page);
  await page.route("**/api/**", (r) =>
    r.fulfill({
      status: 401,
      json: { code: "not_authenticated", message: "sign in to continue" },
    }),
  );
  await page.goto("/");
  await page.getByRole("link", { name: "Sign in with Nextcloud" }).waitFor();

  // Replaced, not covered: a rendered app behind it would already have fetched.
  await expect(page.locator("mat-toolbar")).toHaveCount(0);

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoStarvedText(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("compare — two renders side by side lay out cleanly @ phone", async ({ page }, testInfo) => {
  // The densest page: picker, transport, five-panel chart, two sets of sliders.
  await mockApi(page);
  await page.goto("/compare");
  await page.getByRole("button", { name: "Render both" }).click();
  await page.locator("app-compare-chart canvas").waitFor();
  await page.evaluate(() => {
    window.scrollTo(0, 0);
  });

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoStarvedText(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("compare — both settings panels open lay out cleanly @ phone", async ({ page }, testInfo) => {
  // Both settings panels open: the grid must drop to one column.
  await mockApi(page);
  await page.goto("/compare");
  await page.getByRole("button", { name: "Change settings" }).click();
  await page.locator("app-mapping-controls mat-slider").last().waitFor();
  await page.evaluate(() => {
    window.scrollTo(0, 0);
  });

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoStarvedText(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

/**
 * Canvas ignores an unparseable colour silently, keeping black — and Material's
 * tokens compute to `light-dark(…)`, which no canvas parses, so black text on a
 * dark background reports nothing. Only reading the pixels can see it.
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
        const [br, bg, bb] = background.match(/\d+/g)?.map(Number) ?? [];
        // A background that did not parse must stop the check, not default to
        // black, against which light marks pass.
        if (br === undefined || bg === undefined || bb === undefined) {
          throw new Error(`body background is not an rgb() colour: ${background}`);
        }
        const backgroundLuminance = relativeLuminance(br, bg, bb);

        const ctx = canvas.getContext("2d")!;
        const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);

        // Solid pixels only: antialiased edges blend toward the background.
        const ratios: number[] = [];
        // RGBA stride 4, so these reads are in bounds.
        const at = (i: number): number => data[i] ?? 0;
        for (let i = 0; i < data.length; i += 4) {
          if (at(i + 3) < 200) continue;
          const l = relativeLuminance(at(i), at(i + 1), at(i + 2));
          const [hi, lo] = l > backgroundLuminance ? [l, backgroundLuminance] : [backgroundLuminance, l];
          ratios.push((hi + 0.05) / (lo + 0.05));
        }
        if (ratios.length === 0) return { painted: 0, best: 0 };
        ratios.sort((a, b) => a - b);
        // `ratios` is non-empty, so the index is in bounds.
        return { painted: ratios.length, best: ratios[Math.floor(ratios.length * 0.9)] ?? 0 };
      });

      expect(contrast.painted, `${selector} painted nothing at all`).toBeGreaterThan(200);
      expect(
        contrast.best,
        `${selector} in ${scheme} mode: brightest marks reach only ${contrast.best.toFixed(1)}:1 against the page`,
      ).toBeGreaterThan(3);
    }
  });
}

/**
 * A comparison is a link. Checked end to end because the failure is in the
 * wiring: the read waits for the published knobs and the write for the read, or
 * a shared link is overwritten with defaults before anyone sees it.
 */
test("compare — a shared link arrives at the settings it names", async ({ page }) => {
  await mockApi(page);
  await page.goto(
    "/compare?take=0123456789abcdef&a=" +
      encodeURIComponent("mapping=tonnetz") +
      "&b=" +
      encodeURIComponent("mapping=tonnetz&bind=0"),
  );

  // What the two sides disagree about, as the page names it.
  const differing = page.locator("p.differing");
  await expect(differing).toContainText("Bind to the voice");
  await expect(differing.locator(".diff", { hasText: "Bind to the voice" })).toContainText("1");
  await expect(differing.locator(".diff", { hasText: "Bind to the voice" })).toContainText("0");
  // Both sides on the lattice, so the mapping is not listed as a difference.
  await expect(differing).not.toContainText("Mapping");

  // And opening the link leaves it unchanged.
  await expect(page).toHaveURL(/a=mapping%3Dtonnetz&b=mapping%3Dtonnetz%26bind%3D0/);
});

/**
 * Primary knobs shown, the rest folded — checked end to end, because the split
 * must come from what the backend published, not a frontend list.
 */
test("studio — the knobs that decide the piece come first, the rest fold away", async ({
  page,
}) => {
  await mockApi(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Render" }).first().waitFor();

  const knobs = page.locator("app-mapping-controls .knob");
  const panel = page.getByRole("button", { name: /More controls/ });

  // The field mapping is playing, so the lattice's `hold` is put away.
  await expect(knobs).toHaveCount(4);
  await expect(panel).toBeVisible();

  await panel.click();
  // The rest are reachable once opened. Counted in the DOM: folded sliders must
  // be absent, not present and sized zero, which the harness reports as
  // occluded.
  await expect(knobs).toHaveCount(7);
});

test("studio — a folded-away knob still says it was moved", async ({ page }) => {
  // Closed, the panel must still say something inside has moved.
  await mockApi(page);
  await page.goto("/");
  const panel = page.getByRole("button", { name: /More controls/ });
  await expect(panel).toContainText("more");

  await panel.click();
  // A knob that adjusts a piece rather than choosing what kind it is.
  const folded = page.locator("app-mapping-controls .knob", { hasText: "Follow the vowel" });
  await folded.locator("input[matSliderThumb]").fill("2");
  await panel.click();

  await expect(panel).toContainText("1 moved");
});





test("the menu reaches every page @ phone", async ({ page }) => {
  // A menu link that looks right and goes nowhere is invisible to layout
  // checks, so it is clicked.
  await mockApi(page);
  await page.goto("/");

  for (const [name, path] of [
    ["Calibrate", "/calibrate"],
    ["Compare", "/compare"],
    ["Studio", "/"],
  ] as const) {
    await page.getByRole("button", { name: "Open the menu" }).click();
    await page.getByRole("menuitem", { name }).click();
    await expect(page).toHaveURL(new RegExp(`${path.replace("/", "\\/")}(\\?|$)`));
  }
});

test("the menu says which page you are on", async ({ page }) => {
  // aria-current rather than a class, so highlight and screen reader agree.
  await mockApi(page);
  await page.goto("/compare");
  await page.getByRole("button", { name: "Open the menu" }).click();

  await expect(page.getByRole("menuitem", { name: "Compare" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(page.getByRole("menuitem", { name: "Studio" })).not.toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("the pages are inline when there is room for them", async ({ page }) => {
  // Widened on purpose: a suite at one width cannot tell responsive from
  // permanently collapsed.
  await mockApi(page);
  await page.setViewportSize({ width: 1200, height: 800 });
  await page.goto("/");

  await expect(page.getByRole("button", { name: "Open the menu" })).toHaveCount(0);
  for (const name of ["Calibrate", "Studio", "Compare"]) {
    await expect(page.getByRole("link", { name, exact: true })).toBeVisible();
  }
  // Marked the same way as in the menu, by the same method.
  await expect(page.getByRole("link", { name: "Studio", exact: true })).toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("studio — with no voice yet, the page offers the way to make one", async ({ page }) => {
  // The next move is offered before anything is refused.
  await mockApi(page);
  await page.route("**/api/recordings", (r) =>
    r.fulfill({ json: [{ ...META, role: "material" }] }),
  );
  await page.goto("/");

  const offer = page.getByRole("link", { name: "Record the calibration vowels" });
  await expect(offer).toBeVisible();
  await expect(offer).toHaveAttribute("href", "/calibrate");
});

test("studio — once there is a voice, it stops asking", async ({ page }) => {
  // ...and only while it is true.
  await mockApi(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Render as music" }).waitFor();

  await expect(
    page.getByRole("link", { name: "Record the calibration vowels" }),
  ).toHaveCount(0);
});

test("a question mark opens the explanation, and it is not there until asked", async ({
  page,
}) => {
  // Deferred: rendered eagerly, a screen reader would read it unprompted.
  await mockApi(page);
  // The studio's one explanation is on the no-voice card.
  await page.route("**/api/recordings", (r) =>
    r.fulfill({ json: [{ ...META, role: "material" }] }),
  );
  await page.goto("/");

  await expect(page.locator(".help-body")).toHaveCount(0);
  await page.getByRole("button", { name: "What is this?" }).first().click();
  await expect(page.locator(".help-body")).toContainText("spectrum of a sustained vowel");
});





