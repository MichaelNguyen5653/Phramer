// The editor page in Edge with the Tauri runtime mocked: reference
// screenshots (light, dark, a drawn shape, the colour picker), no layout
// shift, and a picker that applies on pointer move with no confirm step.

import { expect, test, type Page } from "@playwright/test";
import { installTauriMock } from "./tauri-mock";

async function open(page: Page) {
  await page.addInitScript(installTauriMock);
  // Layout shifts, recorded from the first paint
  await page.addInitScript(() => {
    (window as unknown as { __shift: number }).__shift = 0;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries() as (PerformanceEntry & { value: number; hadRecentInput: boolean })[]) {
        (window as unknown as { __shift: number }).__shift += entry.value;
      }
    }).observe({ type: "layout-shift", buffered: true });
  });
  await page.goto("/editor.html");
  await expect(page.getByTestId("editor")).toBeVisible();
  // The daemon is told once the image is drawn
  await expect.poll(() => page.evaluate(() => window.__calls.some((call) => call.cmd === "editor_ready"))).toBe(true);
}

const calls = (page: Page, cmd: string) =>
  page.evaluate((cmd) => window.__calls.filter((call) => call.cmd === cmd).map((call) => call.args), cmd);

const boxes = (page: Page) =>
  page.evaluate(() =>
    ["toolbar", "viewport", "options-panel", "statusbar"].map((id) => {
      const box = document.querySelector(`[data-testid="${id}"]`)?.getBoundingClientRect();
      return box ? [box.x, box.y, box.width, box.height] : null;
    }),
  );

test("idle, light", async ({ page }) => {
  await open(page);
  await expect(page.getByTestId("zoom")).toHaveText("100%");
  await expect(page).toHaveScreenshot("editor-idle-light.png");
});

test("idle, dark", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await open(page);
  await expect(page).toHaveScreenshot("editor-idle-dark.png");
});

test("a drawn rectangle", async ({ page }) => {
  await open(page);
  await page.keyboard.press("r");
  await expect(page.getByTestId("tool-rectangle")).toHaveAttribute("aria-pressed", "true");
  const canvas = await page.getByTestId("canvas").boundingBox();
  if (!canvas) throw new Error("no canvas");
  await page.mouse.move(canvas.x + 80, canvas.y + 90);
  await page.mouse.down();
  await page.mouse.move(canvas.x + 200, canvas.y + 150, { steps: 4 });
  await page.mouse.move(canvas.x + 300, canvas.y + 220, { steps: 4 });
  await page.mouse.up();
  await expect(page.getByTestId("action-undo")).toBeEnabled();
  await expect(page).toHaveScreenshot("editor-shape.png");
  // Undo takes it away again
  await page.keyboard.press("Control+z");
  await expect(page.getByTestId("action-undo")).toBeDisabled();
});

test("the colour picker applies on pointer move, with no confirm control", async ({ page }) => {
  await open(page);
  const before = await boxes(page);
  await page.getByTestId("color-button").click();
  const picker = page.getByTestId("color-picker");
  await expect(picker).toBeVisible();
  await expect(page).toHaveScreenshot("editor-picker.png");

  // No OK, Apply or Done anywhere in it
  await expect(picker.getByRole("button", { name: /^(ok|apply|done|confirm)$/i })).toHaveCount(0);

  const square = await page.getByTestId("picker-square").boundingBox();
  if (!square) throw new Error("no square");
  const current = page.getByTestId("picker-current");
  const panelHex = page.getByTestId("color-button");
  await page.mouse.move(square.x + square.width * 0.9, square.y + square.height * 0.1);
  await page.mouse.down();
  const first = await current.evaluate((el) => getComputedStyle(el).backgroundColor);
  // Each move applies while the button is still held: no release, no OK
  const seen = new Set([first]);
  for (const [fx, fy] of [
    [0.2, 0.2],
    [0.5, 0.6],
    [0.8, 0.3],
  ]) {
    await page.mouse.move(square.x + square.width * fx, square.y + square.height * fy);
    const now = await current.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(seen.has(now)).toBe(false);
    seen.add(now);
    // The panel follows at once too
    await expect(panelHex).toContainText((await page.getByTestId("picker-hex").inputValue()).toLowerCase());
  }
  await page.mouse.up();
  // The pick is saved to drawColor once, at the end of the drag
  await expect.poll(async () => (await calls(page, "editor_set")).length).toBe(1);
  const picked = await page.getByTestId("picker-hex").inputValue();

  // Esc closes it and keeps the colour
  await page.keyboard.press("Escape");
  await expect(picker).toBeHidden();
  await expect(panelHex).toContainText(picked);

  // A palette swatch applies on press, and a click away keeps it
  await page.getByTestId("color-button").click();
  await page.getByRole("option", { name: "#0000ff" }).dispatchEvent("pointerdown", { button: 0 });
  await expect(panelHex).toContainText("#0000ff");
  await page.getByTestId("viewport").click({ position: { x: 20, y: 20 } });
  await expect(picker).toBeHidden();
  await expect(panelHex).toContainText("#0000ff");

  // Undo takes the pick back
  await page.keyboard.press("Control+z");
  await expect(panelHex).toContainText(picked);

  // Opening and closing the picker moved nothing
  expect(await boxes(page)).toEqual(before);
});

test("no layout shift while working", async ({ page }) => {
  await open(page);
  const before = await boxes(page);
  for (const tool of ["pencil", "arrow", "shape", "text", "circle_count", "pixelate"]) {
    await page.getByTestId(`tool-${tool}`).click();
    expect(await boxes(page)).toEqual(before);
  }
  await page.getByTestId("action-zoom_in").click();
  await page.getByTestId("action-fit").click();
  await page.getByTestId("color-button").click();
  await page.keyboard.press("Escape");
  expect(await boxes(page)).toEqual(before);
  const shift = await page.evaluate(() => (window as unknown as { __shift: number }).__shift);
  expect(shift).toBe(0);
});

test("reduced motion turns transitions off", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await open(page);
  const duration = await page
    .getByTestId("tool-pencil")
    .evaluate((el) => getComputedStyle(el).transitionDuration.split(",").map((part) => parseFloat(part)));
  expect(duration.every((value) => value === 0)).toBe(true);
});

test("exports send the rendered image to the daemon", async ({ page }) => {
  await open(page);
  await page.keyboard.press("Control+c");
  await expect.poll(async () => (await calls(page, "editor_export")).length).toBe(1);
  await expect(page.getByTestId("statusbar")).toContainText("Capture saved to clipboard.");
});

test("text is typed on the image and placed with Esc", async ({ page }) => {
  await open(page);
  await page.getByTestId("tool-text").click();
  const canvas = await page.getByTestId("canvas").boundingBox();
  if (!canvas) throw new Error("no canvas");
  await page.mouse.click(canvas.x + 100, canvas.y + 120);
  await expect(page.getByRole("textbox", { name: "Text" })).toBeFocused();
  await page.keyboard.type("Hello");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("action-undo")).toBeEnabled();
  // The tool shortcuts work again once the text is placed
  await page.keyboard.press("p");
  await expect(page.getByTestId("tool-pencil")).toHaveAttribute("aria-pressed", "true");
});
