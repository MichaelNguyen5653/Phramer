// The OCR results window in Edge with the daemon mocked: reference
// screenshots (light and dark, busy), the layout applied live without a new
// recognition, the language picker, Try harder, exact text, failures, Copy
// all and Esc.

import { expect, test, type Page } from "@playwright/test";
import { installPanelMock, type PanelMockOptions } from "./panel-mock";

const recognized = {
  ok: true,
  message: null,
  exact: false,
  diagnostics: "capture 640x200\npass A: scale 1.00, 1 tile(s), score 92\nkept score 92",
  texts: [
    "fn main() {\n    let total = 42;\n    println!(\"{total}\");\n}\n\nThe quick brown fox jumps over\nthe lazy dog.",
    "fn main() {\nlet total = 42;\nprintln!(\"{total}\");\n}\nThe quick brown fox jumps over\nthe lazy dog.",
    "fn main() {\n    let total = 42;\n    println!(\"{total}\");\n}\n\nThe quick brown fox jumps over the lazy dog.",
  ],
};
const harder = { ...recognized, diagnostics: "pass C (binarized)", texts: ["harder", "harder plain", "harder join"] };
const languages = [
  { tag: "en-US", name: "English (United States)" },
  { tag: "de-DE", name: "Deutsch (Deutschland)" },
];

async function openOcr(page: Page, options: Partial<PanelMockOptions> = {}) {
  await page.addInitScript(installPanelMock, { label: "ocr-1", normal: recognized, high: harder, languages, ...options });
  await page.goto("/ocr.html");
  await expect(page.getByTestId("ocr")).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.__calls.some((call) => call.cmd === "ui_ready"))).toBe(true);
}

const calls = (page: Page, cmd: string) =>
  page.evaluate((cmd) => window.__calls.filter((call) => call.cmd === cmd).map((call) => call.args), cmd);

test.use({ viewport: { width: 520, height: 420 } });

for (const scheme of ["light", "dark"] as const) {
  test(`a recognised result (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await openOcr(page);
    await expect(page.getByTestId("ocr-text")).toHaveValue(recognized.texts[0]);
    await expect(page.getByTestId("ocr-language")).toBeVisible();
    await expect(page.getByTestId("ocr-hint")).toHaveText(
      "Recognized from the captured image. Capture close up and unblurred for the best result.",
    );
    await expect(page).toHaveScreenshot(`ocr-result-${scheme}.png`);
  });
}

test("the layout applies live and is remembered, without recognising again", async ({ page }) => {
  await openOcr(page, { layout: 1 });
  await expect(page.getByTestId("ocr-text")).toHaveValue(recognized.texts[1]);
  await page.getByTestId("ocr-layout").locator("select").selectOption({ label: "Join wrapped" });
  await expect(page.getByTestId("ocr-text")).toHaveValue(recognized.texts[2]);
  expect(await calls(page, "ocr_set_layout")).toEqual([{ layout: 2 }]);
  expect(await calls(page, "ocr_recognize")).toHaveLength(1);
});

test("the busy indicator waits 200 ms, then says what is happening", async ({ page }) => {
  await openOcr(page, { delay: 1500 });
  await expect(page.getByTestId("ocr-status")).toHaveCount(0);
  await expect(page.getByTestId("ocr-status")).toHaveText("Recognizing text…");
  await expect(page.getByTestId("ocr-try-harder")).toBeDisabled();
  await expect(page.getByTestId("ocr-hint")).toHaveText(
    "To improve OCR result, capture image close up and make sure image is not blurry for best result",
  );
  await expect(page).toHaveScreenshot("ocr-busy.png");
  await expect(page.getByTestId("ocr-text")).toHaveValue(recognized.texts[0]);
  await expect(page.getByTestId("ocr-status")).toHaveCount(0);
});

test("Try harder recognises again at high effort", async ({ page }) => {
  await openOcr(page);
  await expect(page.getByTestId("ocr-try-harder")).toBeEnabled();
  await expect(page.getByTestId("ocr-try-harder")).toHaveAttribute(
    "title",
    "Recognize again with every strategy the tool has, including thresholding and a larger upscale. Slower.",
  );
  await page.getByTestId("ocr-try-harder").click();
  await expect(page.getByTestId("ocr-text")).toHaveValue("harder");
  expect(await calls(page, "ocr_recognize")).toEqual([
    { language: "", effort: "normal" },
    { language: "", effort: "high" },
  ]);
});

test("exact text says so and cannot be tried harder", async ({ page }) => {
  await openOcr(page, { normal: { ...recognized, exact: true } });
  await expect(page.getByTestId("ocr-hint")).toHaveText("Exact text, read from the window itself.");
  await expect(page.getByTestId("ocr-try-harder")).toBeDisabled();
});

test("no text found is reported, and Try harder is offered", async ({ page }) => {
  await openOcr(page, {
    normal: { ok: false, message: "No text was found in the selection.", exact: false, diagnostics: "", texts: ["", "", ""] },
  });
  await expect(page.getByTestId("ocr-status")).toHaveText("No text was found in the selection.");
  await expect(page.getByTestId("ocr-try-harder")).toBeEnabled();
  await expect(page.getByTestId("ocr-copy")).toBeDisabled();
});

test("with one language there is no picker", async ({ page }) => {
  await openOcr(page, { languages: [languages[0]] });
  await expect(page.getByTestId("ocr-text")).toHaveValue(recognized.texts[0]);
  await expect(page.getByTestId("ocr-language")).toHaveCount(0);
});

test("choosing a language recognises with it and is remembered", async ({ page }) => {
  await openOcr(page, { language: "fr-FR" });
  // fr-FR is no longer installed: System default
  await expect(page.getByTestId("ocr-language").locator("select")).toHaveValue("");
  await page.getByTestId("ocr-language").locator("select").selectOption("de-DE");
  await expect.poll(() => calls(page, "ocr_recognize")).toHaveLength(2);
  expect((await calls(page, "ocr_recognize"))[1]).toEqual({ language: "de-DE", effort: "normal" });
  expect(await calls(page, "ocr_set_language")).toEqual([{ language: "de-DE" }]);
});

test("Copy all copies the text as edited, and says so", async ({ page }) => {
  await openOcr(page);
  await page.getByTestId("ocr-text").fill("edited by hand");
  await page.getByTestId("ocr-copy").click();
  await expect(page.getByTestId("ocr-copy")).toHaveText("Copied");
  expect(await calls(page, "ocr_copy")).toEqual([{ text: "edited by hand" }]);
  await expect(page.getByTestId("ocr-copy")).toHaveText("Copy all");
});

test("nothing is copied unless asked", async ({ page }) => {
  await openOcr(page);
  await expect(page.getByTestId("ocr-text")).toHaveValue(recognized.texts[0]);
  expect(await calls(page, "ocr_copy")).toEqual([]);
});

test("Esc closes the window", async ({ page }) => {
  await openOcr(page);
  await page.keyboard.press("Escape");
  await expect.poll(() => calls(page, "plugin:window|close")).toHaveLength(1);
});
