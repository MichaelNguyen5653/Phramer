// The editor page in Edge with the Tauri runtime mocked: reference
// screenshots (light, dark, a drawn shape, the colour picker, the toolbar's
// keys, a narrow window, the filmstrip, the close prompt, the grid, the
// empty editor), no layout shift, and a picker that applies on pointer move
// with no confirm step.

import { expect, test, type Page } from "@playwright/test";
import { installTauriMock, type MockOptions } from "./tauri-mock";

async function open(page: Page, options: MockOptions = {}) {
  await page.addInitScript(installTauriMock, options);
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

const hint = (page: Page, testId: string) => page.getByTestId(testId).locator(".key-hint");

async function drawRectangle(page: Page, from: [number, number], to: [number, number]) {
  await page.keyboard.press("r");
  const canvas = await page.getByTestId("canvas").boundingBox();
  if (!canvas) throw new Error("no canvas");
  await page.mouse.move(canvas.x + from[0], canvas.y + from[1]);
  await page.mouse.down();
  await page.mouse.move(canvas.x + to[0], canvas.y + to[1], { steps: 4 });
  await page.mouse.up();
  await page.keyboard.press("Escape");
}

test("toolbar keys: in brackets when assigned, nothing when not, live", async ({ page }) => {
  await open(page);
  await expect(hint(page, "tool-pencil")).toHaveText("(P)");
  await expect(hint(page, "action-undo")).toHaveText("(Ctrl+Z)");
  await expect(hint(page, "action-redo")).toHaveText("(Ctrl+Shift+Z)");
  await expect(hint(page, "action-copy_file")).toHaveText("(Ctrl+Shift+C)");
  // Unassigned in [Shortcuts], or no entry at all: no hint and no brackets
  for (const id of ["tool-shape", "tool-circle_count", "action-pin", "action-crop", "action-save_as", "action-fit"]) {
    await expect(hint(page, id)).toHaveCount(0);
    expect(await page.getByTestId(id).textContent()).not.toContain("(");
  }
  // The tooltip keeps the full description and its key
  await expect(page.getByTestId("tool-pencil")).toHaveAttribute("title", "Set the Pencil as the paint tool (P)");
  // A key set or cleared in Settings shows at once
  await page.evaluate(() => {
    window.__setShortcut("TYPE_SHAPE", "H");
    window.__setShortcut("TYPE_PENCIL", "");
  });
  await expect(hint(page, "tool-shape")).toHaveText("(H)");
  await expect(hint(page, "tool-pencil")).toHaveCount(0);
  await page.keyboard.press("h");
  await expect(page.getByTestId("tool-shape")).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("toolbar")).toHaveScreenshot("editor-toolbar-keys.png");
});

test("a narrow window wraps the toolbar: every button and key visible, nothing scrolls", async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 520 });
  await open(page);
  const toolbar = page.getByTestId("toolbar");
  const overflow = await toolbar.evaluate((el) => el.scrollWidth - el.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  const outside = await page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>(".toolbar .tool")]
      .map((button) => ({ id: button.dataset.testid, box: button.getBoundingClientRect() }))
      .filter(({ box }) => box.right > window.innerWidth || box.left < 0 || box.width === 0)
      .map(({ id }) => id),
  );
  expect(outside).toEqual([]);
  // The image still has room
  const viewport = await page.getByTestId("viewport").boundingBox();
  expect(viewport!.height).toBeGreaterThan(200);
  await expect(page).toHaveScreenshot("editor-narrow.png");
});

test("filmstrip: images join, each keeps its own history, and the strip shows them", async ({ page }) => {
  await open(page);
  // One image: no strip
  await expect(page.getByTestId("filmstrip")).toHaveCount(0);
  await expect(page.getByTestId("position")).toHaveCount(0);
  await page.evaluate(() => {
    window.__addImage(1);
    window.__addImage(2);
  });
  await expect(page.getByTestId("filmstrip")).toBeVisible();
  await expect(page.getByTestId("thumb-3")).toHaveAttribute("aria-current", "true");
  await expect(page.getByTestId("position")).toHaveText("Image 3 of 3");
  // The daemon is told each joined image is drawn, so it can bring the window up
  await expect.poll(async () => (await calls(page, "editor_ready")).length).toBe(3);
  await drawRectangle(page, [80, 90], [260, 200]);
  await expect(page.getByTestId("action-undo")).toBeEnabled();
  await page.getByTestId("thumb-1").click();
  await expect(page.getByTestId("position")).toHaveText("Image 1 of 3");
  await expect(page.getByTestId("action-undo")).toBeDisabled();
  await drawRectangle(page, [300, 120], [420, 260]);
  await page.getByTestId("thumb-2").click();
  await expect(page.getByTestId("action-undo")).toBeDisabled();
  await page.getByTestId("thumb-3").click();
  await expect(page.getByTestId("action-undo")).toBeEnabled();
  await expect(page.getByTestId("unsaved")).toHaveCount(2);
  await expect(page.locator(".thumb img")).toHaveCount(3);
  await page.waitForTimeout(400);
  await expect(page).toHaveScreenshot("editor-filmstrip.png");

  // Closing names the images that are not saved
  await page.getByRole("button", { name: "Close" }).click();
  const confirm = page.getByTestId("confirm");
  await expect(confirm).toContainText(
    "Images 1 and 3 of 3 have changes that are not saved. Closing the editor discards every image in it.",
  );
  await expect(page).toHaveScreenshot("editor-close-prompt.png");
  await confirm.getByRole("button", { name: "Keep editing" }).click();
  await expect(confirm).toBeHidden();

  // Save All: one folder, every image numbered, all saved after
  await page.getByTestId("save-all").click();
  await expect(page.getByTestId("statusbar")).toContainText("3 images saved to C:\\Pictures\\Phramer set");
  const exports = await page.evaluate(() =>
    window.__calls.filter((call) => call.cmd === "editor_export").map((call) => call.headers),
  );
  expect(exports.map((headers) => [headers?.["x-action"], headers?.["x-number"]])).toEqual([
    ["save_all", "1"],
    ["save_all", "2"],
    ["save_all", "3"],
  ]);
  expect((await calls(page, "editor_save_all_begin")).length).toBe(1);
  expect((await calls(page, "editor_save_all_end")).length).toBe(1);
  await expect(page.getByTestId("unsaved")).toHaveCount(0);
  // Nothing unsaved: Close closes at once
  await page.getByRole("button", { name: "Close" }).click();
  await expect.poll(async () => (await calls(page, "plugin:window|destroy")).length).toBe(1);
});

test("Remove Image asks for an unsaved image, and the strip goes with the second-last image", async ({ page }) => {
  await open(page);
  await page.evaluate(() => window.__addImage(1));
  await expect(page.getByTestId("thumb-2")).toHaveAttribute("aria-current", "true");
  await drawRectangle(page, [80, 90], [260, 200]);
  await page.getByTestId("remove-image").click();
  const confirm = page.getByTestId("confirm");
  await expect(confirm).toContainText("Image 2 of 2 has changes that are not saved.");
  await confirm.getByRole("button", { name: "Remove without saving" }).click();
  await expect(page.getByTestId("filmstrip")).toHaveCount(0);
  await expect(page.getByTestId("action-undo")).toBeDisabled();
  // A clean image goes without a question
  await page.evaluate(() => window.__addImage(2));
  await expect(page.getByTestId("thumb-2")).toBeVisible();
  await page.getByTestId("thumb-1").click();
  await page.getByTestId("remove-image").click();
  await expect(page.getByTestId("confirm")).toHaveCount(0);
  await expect(page.getByTestId("filmstrip")).toHaveCount(0);
});

test("grid and Zoom mode, as in 15.0.0", async ({ page }) => {
  await open(page);
  await expect(page.getByTestId("grid")).toHaveCount(0);
  await page.keyboard.press("Control+Quote");
  await expect(page.getByTestId("grid")).toBeVisible();
  await expect(page.getByTestId("action-grid")).toHaveAttribute("aria-pressed", "true");
  await expect(page).toHaveScreenshot("editor-grid.png");
  // Z: the plain wheel zooms at the pointer; Esc puts it down
  await page.keyboard.press("p");
  await page.keyboard.press("z");
  await expect(page.getByTestId("action-zoom_mode")).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("tool-pencil")).toHaveAttribute("aria-pressed", "false");
  const canvas = await page.getByTestId("canvas").boundingBox();
  await page.mouse.move(canvas!.x + 100, canvas!.y + 100);
  await page.mouse.wheel(0, -120);
  await expect(page.getByTestId("zoom")).toHaveText("125%");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("action-zoom_mode")).toHaveAttribute("aria-pressed", "false");
  // Without it the plain wheel pans again
  await page.mouse.wheel(0, -120);
  await expect(page.getByTestId("zoom")).toHaveText("125%");
  await page.getByTestId("action-grid").click();
  await expect(page.getByTestId("grid")).toHaveCount(0);
});

test("digit keys type the tool size, as on the overlay", async ({ page }) => {
  await open(page);
  await page.keyboard.press("p");
  await page.keyboard.press("1");
  await page.keyboard.press("2");
  await expect(page.getByTestId("size-slider")).toHaveValue("12");
  // Kept once the digits stop
  await expect
    .poll(async () => (await calls(page, "editor_set")).map((args) => JSON.stringify(args)))
    .toContain(JSON.stringify({ change: { key: "tool_size", tool: "pencil", size: 12 } }));
  await page.keyboard.press("9");
  await expect(page.getByTestId("size-slider")).toHaveValue("9");
});

test("a placed text opens again on double-click, with bold, italic and font", async ({ page }) => {
  await open(page);
  await page.getByTestId("tool-text").click();
  const font = page.getByTestId("font-family");
  await expect(font.locator("option")).toHaveCount(4);
  await font.selectOption("Cambria");
  await expect.poll(async () => (await calls(page, "editor_font_faces")).length).toBe(1);
  await expect
    .poll(async () => (await calls(page, "editor_set")).map((args) => JSON.stringify(args)))
    .toContain(JSON.stringify({ change: { key: "font_family", family: "Cambria" } }));
  await page.getByRole("checkbox", { name: "Bold" }).check();
  const canvas = await page.getByTestId("canvas").boundingBox();
  if (!canvas) throw new Error("no canvas");
  await page.mouse.click(canvas.x + 100, canvas.y + 120);
  await page.keyboard.type("Hello");
  await page.keyboard.press("Escape");
  // No tool, then a double-click on the text
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("tool-text")).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByRole("checkbox", { name: "Bold" })).toHaveCount(0);
  await page.mouse.dblclick(canvas.x + 112, canvas.y + 128);
  await expect(page.getByRole("textbox", { name: "Text" })).toBeFocused();
  await expect(page.getByTestId("options-panel")).toContainText("Text");
  await expect(page.getByRole("checkbox", { name: "Bold" })).toBeChecked();
  await page.keyboard.type("!");
  await page.keyboard.press("Escape");
  // Placing and the edit: two steps back to nothing
  await page.keyboard.press("Control+z");
  await expect(page.getByTestId("action-undo")).toBeEnabled();
  await page.keyboard.press("Control+z");
  await expect(page.getByTestId("action-undo")).toBeDisabled();
});

test("the empty editor says how to capture, and the first image replaces it", async ({ page }) => {
  await page.addInitScript(installTauriMock, { empty: true });
  await page.goto("/editor.html");
  await expect(page.getByTestId("empty-hint")).toHaveText(
    "Press Shift+Win+X or Print Screen to capture. Then pick Editor (E) on the capture toolbar.",
  );
  await expect.poll(async () => (await calls(page, "editor_ready")).length).toBe(1);
  await expect(page).toHaveScreenshot("editor-empty.png");
  await page.evaluate(() => window.__addImage(0));
  await expect(page.getByTestId("editor")).toBeVisible();
  await expect.poll(async () => (await calls(page, "editor_ready")).length).toBe(2);
  await expect(page.getByTestId("filmstrip")).toHaveCount(0);
});
