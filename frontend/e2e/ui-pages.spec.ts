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
      // Tonal inside the bursts, noisy in the gaps — the shape a consonant
      // makes, so the fixture exercises the same fields a real take does.
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
  // Present and null, not absent. A mock missing a field the wire really
  // carries makes every assertion about that field pass by not running.
  refusal: null,
};

/** What the backend says when the chosen mapping has no answer for this scale. */
const REFUSAL =
  "Lattice cannot be played in this scale: this voice's scale has one interval " +
  "(702¢) besides the tonic and the octave, and a lattice is spanned by two " +
  "intervals pointing different ways. Lowering the scale density keeps more of them.";

/**
 * The mapping's knobs, as the backend publishes them.
 *
 * Copied from `utterance_mapping::params::KNOBS` rather than fetched, because this
 * suite is about layout: what matters is that a column of sliders, a toggle
 * group and a select fit on a phone, not that these are the current ranges.
 * Drift in the numbers costs nothing — they are checked against the mapping in
 * `tests/api.rs`, where getting them wrong actually matters.
 *
 * The `mappings` on each knob is not decoration here: the controls hide a knob
 * the playing mapping does not read, so a mock without it renders no sliders at
 * all and every layout assertion below passes over an empty page.
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
 * A score, as the compare page charts it.
 *
 * Two of these are drawn on one axis, so what matters for layout is that the
 * series are long enough for the canvas to fill and the degrees long enough to
 * push the scale caption onto a second line on a phone.
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
  // Trailing wildcard because the summary now carries the mapping settings in
  // its query string, and a glob without one stops matching the moment a
  // parameter is added — silently, by falling through to the catch-all above.
  await page.route("**/api/voice*", (r) => r.fulfill({ json: VOICE }));
  await page.route("**/api/controls", (r) => r.fulfill({ json: CONTROLS }));
  // Both sides of the comparison, told apart by the query so the two charts are
  // genuinely different curves rather than one drawn twice.
  await page.route("**/score*", (r) =>
    r.fulfill({ json: score(r.request().url().includes("bind=0") ? 2 : 0) }),
  );
}

/**
 * A strip beside the sliders that a thumb can land on without turning a knob.
 *
 * A Material slider takes its value from where a pointer goes down, before the
 * browser has decided the gesture was a scroll — so on a phone a full-width
 * column of them has no safe place to start a scroll, and reading down the page
 * quietly changes the settings. The fix is layout, so this is where it is
 * guarded: assert the gutter exists rather than trust a `calc()` nobody reads.
 *
 * Every slider is checked, not the first: the failure that matters is one knob
 * reaching the edge, and that is exactly the one a spot check misses.
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
    // Against the 44 px that touch guidance settles on for a thumb, less a
    // little for rounding and the slider's own end padding.
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
  // Two canvases now — the time-series chart and the vowel space. Wait for
  // both, so the layout assertions run against the fully painted page.
  await page.locator("app-voiceprint-chart canvas").waitFor();
  await page.locator("app-vowel-space canvas").waitFor();
  // The knobs are the densest thing on the page — a column of sliders, a toggle
  // group and a select — and a phone is where they are likeliest to collide.
  await page.locator("app-mapping-controls mat-slider").last().waitFor();

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
  await expectSomewhereToScrollFrom(page);
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

  // Clicking auto-scrolled the button into view, which leaves content under the
  // sticky toolbar. The toolbar is opaque, so that is not a visual defect — but
  // the overlap check measures geometry and cannot know that. Return to the top
  // so the assertions describe the layout rather than the scroll position.
  await page.evaluate(() => {
    window.scrollTo(0, 0);
  });

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("studio — a scale that carries no lattice says so @ phone", async ({ page }, testInfo) => {
  // The state this replaced was a player that produced consonants and silence,
  // which reads as a broken build rather than as a setting to move. It is a
  // paragraph of prose in a page otherwise made of numbers and controls, so it
  // is also the likeliest thing here to overflow a phone.
  await mockApi(page);
  await page.route("**/api/voice*", (r) => r.fulfill({ json: { ...VOICE, refusal: REFUSAL } }));
  await page.goto("/");
  await page.getByRole("button", { name: "Render as music" }).click();
  await page.getByRole("alert").filter({ hasText: "Lattice cannot be played" }).waitFor();

  // No player for the *derived* music: pointing one at a render the backend
  // refuses gives a broken control and no reason, which is the failure this
  // whole message exists for. Scoped to the component, because the page also
  // carries a player for the recording itself and that one plays fine.
  await expect(page.locator("app-derived-music audio.player")).toHaveCount(0);

  await page.evaluate(() => {
    window.scrollTo(0, 0);
  });
  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("the sign-in wall lays out cleanly @ phone", async ({ page }, testInfo) => {
  // The deployed app answers 401 until someone signs in with Nextcloud, so this
  // is the first thing anyone sees off the LAN — and it is a card centred in a
  // viewport, which is a layout nothing else in this app has.
  await mockApi(page);
  await page.route("**/api/**", (r) =>
    r.fulfill({
      status: 401,
      json: { code: "not_authenticated", message: "sign in to continue" },
    }),
  );
  await page.goto("/");
  await page.getByRole("link", { name: "Sign in with Nextcloud" }).waitFor();

  // Replaced, not covered. A toolbar still on screen would mean the app behind
  // it rendered, and a page that rendered is a page that fetched.
  await expect(page.locator("mat-toolbar")).toHaveCount(0);

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("compare — two renders side by side lay out cleanly @ phone", async ({ page }, testInfo) => {
  // The densest page in the app: a take picker, a transport, a five-panel chart
  // and two full sets of sliders. A phone is where it collides first.
  await mockApi(page);
  await page.goto("/compare");
  await page.getByRole("button", { name: "Render both" }).click();
  await page.locator("app-compare-chart canvas").waitFor();
  await page.evaluate(() => {
    window.scrollTo(0, 0);
  });

  await expectNoTextOverlaps(page, testInfo);
  await expectNoHorizontalOverflow(page, testInfo);
  await expectNoOccludedControls(page, testInfo);
});

test("compare — both settings panels open lay out cleanly @ phone", async ({ page }, testInfo) => {
  // Two `app-mapping-controls` side by side is nine sliders twice over, and the
  // grid has to drop to one column rather than squeezing both in.
  await mockApi(page);
  await page.goto("/compare");
  await page.getByRole("button", { name: "Change settings" }).click();
  await page.locator("app-mapping-controls mat-slider").last().waitFor();
  await page.evaluate(() => {
    window.scrollTo(0, 0);
  });

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

/**
 * A comparison is the project's unit of evidence, and until it could be linked
 * it could only be passed on as a description of which controls to move. Two
 * people in two rooms then listen to two slightly different things and disagree
 * about a result neither of them heard.
 *
 * Checked here rather than in a unit test because the failure is in the wiring
 * and not in the parsing: the read waits for the published knobs and the write
 * must not run before the read, and getting that order wrong overwrites a shared
 * link with this page's own defaults before anyone sees it. Nothing below the
 * component can see that happen.
 */
test("compare — a shared link arrives at the settings it names", async ({ page }) => {
  await mockApi(page);
  await page.goto(
    "/compare?take=0123456789abcdef&a=" +
      encodeURIComponent("mapping=tonnetz") +
      "&b=" +
      encodeURIComponent("mapping=tonnetz&bind=0"),
  );

  // The one thing the two sides disagree about, as the page itself names it.
  const differing = page.locator("p.differing");
  await expect(differing).toContainText("Bind to the voice");
  await expect(differing.locator(".diff", { hasText: "Bind to the voice" })).toContainText("1");
  await expect(differing.locator(".diff", { hasText: "Bind to the voice" })).toContainText("0");
  // Both sides on the lattice, so the mapping is not listed as a difference.
  await expect(differing).not.toContainText("Mapping");

  // And the link survives being opened: the page writes its own state back, so
  // a URL that changed on arrival would mean the address bar no longer
  // described what is playing.
  await expect(page).toHaveURL(/a=mapping%3Dtonnetz&b=mapping%3Dtonnetz%26bind%3D0/);
});

/**
 * Ten sliders at equal weight is an instrument panel for someone who already
 * knows what each one does. To anybody else — which is to say, to the second
 * person this was built for — it reads as ten things they might be getting
 * wrong.
 *
 * Checked end to end rather than on the component, because the property worth
 * guarding is that the split is driven by what the *backend* published: a
 * frontend that kept its own list of important knobs would pass a unit test and
 * still hide a knob added in Rust.
 */
test("studio — the knobs that decide the piece come first, the rest fold away", async ({
  page,
}) => {
  await mockApi(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Render" }).first().waitFor();

  const knobs = page.locator("app-mapping-controls .knob");
  const panel = page.getByRole("button", { name: /More controls/ });

  // The field mapping is playing, so `hold` is put away as belonging to the
  // lattice — leaving the four primaries it does read.
  await expect(knobs).toHaveCount(4);
  await expect(panel).toBeVisible();

  await panel.click();
  // Everything the playing mapping reads is reachable, just not all at once:
  // the four above plus spacing, drift and consonants. Counted in the DOM
  // rather than by visibility, because a folded panel must not merely hide its
  // sliders — one that is present and sized zero is what the layout harness
  // reports as an occluded control, and it is what `.last()` waits forever for.
  await expect(knobs).toHaveCount(7);
});

test("studio — a folded-away knob still says it was moved", async ({ page }) => {
  // Closed, the panel would otherwise hide that something inside is no longer at
  // its default — and a render nobody can explain then has its cause one click
  // away and invisible.
  await mockApi(page);
  await page.goto("/");
  const panel = page.getByRole("button", { name: /More controls/ });
  await expect(panel).toContainText("more");

  await panel.click();
  // Any knob inside the panel will do; this one is there because following
  // the vowel adjusts a piece rather than deciding what kind it is.
  const folded = page.locator("app-mapping-controls .knob", { hasText: "Follow the vowel" });
  await folded.locator("input[matSliderThumb]").fill("2");
  await panel.click();

  await expect(panel).toContainText("1 moved");
});





test("the menu reaches every page @ phone", async ({ page }) => {
  // Four destinations behind one button. The failure worth guarding is not that
  // the menu opens but that a link inside it still navigates — a menu item that
  // looks right and goes nowhere is indistinguishable from a broken app, and no
  // layout assertion can see it.
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
  // Marked with aria-current rather than a class, so the thing a screen reader
  // reads and the thing that is highlighted cannot drift apart.
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
  // The suite runs at phone geometry, so this one widens the window on purpose:
  // the collapse is the behaviour under test, and a suite that only ever saw one
  // width could not tell a responsive bar from a permanently collapsed one.
  await mockApi(page);
  await page.setViewportSize({ width: 1200, height: 800 });
  await page.goto("/");

  await expect(page.getByRole("button", { name: "Open the menu" })).toHaveCount(0);
  for (const name of ["Calibrate", "Studio", "Compare"]) {
    await expect(page.getByRole("link", { name, exact: true })).toBeVisible();
  }
  // And the current one is marked the same way it is in the menu, from the same
  // component method — so the two renderings cannot disagree about where you are.
  await expect(page.getByRole("link", { name: "Studio", exact: true })).toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("studio — with no voice yet, the page offers the way to make one", async ({ page }) => {
  // A page that knows the next move offers it, and offers it *before* anything
  // is refused: a prompt that appears only after somebody presses render is a
  // prompt most people never see.
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
  // The other half, and the one that keeps it from being nagging: the prompt is
  // shown only while it is true.
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
  // The point of the control: a page says the short true thing, and the longer
  // version exists only for somebody who wants it. Rendered eagerly it would sit
  // in the page for a screen reader to read out unprompted.
  await mockApi(page);
  // The one explanation on the studio belongs to the no-voice card, so this
  // needs a store with nothing that defines the speaker.
  await page.route("**/api/recordings", (r) =>
    r.fulfill({ json: [{ ...META, role: "material" }] }),
  );
  await page.goto("/");

  await expect(page.locator(".help-body")).toHaveCount(0);
  await page.getByRole("button", { name: "What is this?" }).first().click();
  await expect(page.locator(".help-body")).toContainText("spectrum of a sustained vowel");
});





