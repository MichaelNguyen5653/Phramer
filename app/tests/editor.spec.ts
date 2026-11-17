// The editor page in Edge with the Tauri runtime mocked: reference
// screenshots (light, dark, a drawn shape, the colour picker, the toolbar's
// keys, a narrow window, the filmstrip, a name being edited, the image
// menu, the close prompt, the grid, the empty editor, a refused format, the
// Export as list), no layout shift, and a picker that applies on pointer
// move with no confirm step.

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

test("a window shown before its image says it is opening, and can be closed", async ({ page }) => {
  await page.addInitScript(installTauriMock, { pendingImage: true });
  await page.goto("/editor.html");
  const loading = page.getByTestId("editor-loading");
  await expect(loading).toBeVisible();
  await expect(loading).toContainText("Opening the image");
  await expect(page).toHaveScreenshot("editor-loading.png");
  // Nothing is ready yet, so the daemon is not told to show anything
  expect(await calls(page, "editor_ready")).toHaveLength(0);
  await loading.getByRole("button", { name: "Close" }).click();
  await expect.poll(async () => (await calls(page, "plugin:window|destroy")).length).toBe(1);
});

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
  // One image: the strip shows it, the status bar does not count it
  await expect(page.getByTestId("filmstrip")).toBeVisible();
  await expect(page.locator(".thumb")).toHaveCount(1);
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
  expect(exports.map((headers) => [headers?.["x-action"], headers?.["x-number"], headers?.["x-name"]])).toEqual([
    ["save_all", "1", "2026-11-14_09-05"],
    ["save_all", "2", "2026-11-14_09-06"],
    ["save_all", "3", "2026-11-14_09-07"],
  ]);
  expect((await calls(page, "editor_save_all_begin")).length).toBe(1);
  expect((await calls(page, "editor_save_all_end")).length).toBe(1);
  await expect(page.getByTestId("unsaved")).toHaveCount(0);
  // Nothing unsaved: Close closes at once
  await page.getByRole("button", { name: "Close" }).click();
  await expect.poll(async () => (await calls(page, "plugin:window|destroy")).length).toBe(1);
});

test("Remove Image asks for an unsaved image, the strip stays for the last one, and the last one out closes", async ({ page }) => {
  await open(page);
  await page.evaluate(() => window.__addImage(1));
  await expect(page.getByTestId("thumb-2")).toHaveAttribute("aria-current", "true");
  await drawRectangle(page, [80, 90], [260, 200]);
  await page.getByTestId("remove-image").click();
  const confirm = page.getByTestId("confirm");
  await expect(confirm).toContainText("Image 2 of 2 has changes that are not saved.");
  await confirm.getByRole("button", { name: "Remove without saving" }).click();
  await expect(page.getByTestId("filmstrip")).toBeVisible();
  await expect(page.locator(".thumb")).toHaveCount(1);
  await expect(page.getByTestId("action-undo")).toBeDisabled();
  // A clean image goes without a question
  await page.evaluate(() => window.__addImage(2));
  await expect(page.getByTestId("thumb-2")).toBeVisible();
  await page.getByTestId("thumb-1").click();
  await page.getByTestId("remove-image").click();
  await expect(page.getByTestId("confirm")).toHaveCount(0);
  await expect(page.locator(".thumb")).toHaveCount(1);
  // The last image out closes the window
  await page.getByTestId("remove-image").click();
  await expect.poll(async () => (await calls(page, "plugin:window|destroy")).length).toBe(1);
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
  await expect(page.getByTestId("filmstrip")).toBeVisible();
  await expect(page.locator(".thumb")).toHaveCount(1);
});

const exportHeaders = (page: Page) =>
  page.evaluate(() => window.__calls.filter((call) => call.cmd === "editor_export").map((call) => call.headers ?? {}));

const stripNames = (page: Page) => page.locator(".thumb-name").allTextContents();

test("one image: the strip shows it with its name, and Save and Save As use the name", async ({ page }) => {
  await open(page);
  await expect(page.getByTestId("filmstrip")).toBeVisible();
  await expect(page.getByTestId("thumb-1")).toHaveAttribute("aria-current", "true");
  await expect(page.getByTestId("name-1")).toHaveText("2026-11-14_09-05.png");
  // The whole name is in the tooltip
  await expect(page.getByTestId("name-1")).toHaveAttribute("title", "2026-11-14_09-05.png");
  await expect(page.locator(".thumb img")).toHaveCount(1);
  await expect(page.getByTestId("save-all")).toBeVisible();
  await expect(page.getByTestId("remove-image")).toBeVisible();
  await page.keyboard.press("Control+s");
  await page.getByTestId("action-save_as").click();
  await page.keyboard.press("Control+c");
  await expect.poll(async () => (await exportHeaders(page)).length).toBe(3);
  // Only the saves carry it
  expect((await exportHeaders(page)).map((headers) => [headers["x-action"], headers["x-name"]])).toEqual([
    ["save", "2026-11-14_09-05"],
    ["save_as", "2026-11-14_09-05"],
    ["copy", undefined],
  ]);
});

test("a long name is cut short under its thumbnail", async ({ page }) => {
  await open(page);
  const long = "A very long name for a screenshot of the login page bug";
  await page.getByTestId("name-1").dblclick();
  await page.keyboard.type(long);
  await page.keyboard.press("Enter");
  const name = page.getByTestId("name-1");
  await expect(name).toHaveText(`${long}.png`);
  await expect(name).toHaveAttribute("title", `${long}.png`);
  // The name is cut short; the extension stays whole
  const stem = name.locator(".thumb-stem");
  expect(await stem.evaluate((el) => el.scrollWidth > el.clientWidth && getComputedStyle(el).textOverflow === "ellipsis")).toBe(true);
  expect(
    await name.evaluate((el) => {
      const ext = el.querySelector(".thumb-ext")!.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      return ext.width > 0 && ext.left >= box.left && ext.right <= box.right + 0.5;
    }),
  ).toBe(true);
});

test("rename in place: Enter keeps, Esc cancels, refused characters, leaving keeps", async ({ page }) => {
  await open(page);
  const before = await boxes(page);
  const name = page.getByTestId("name-1");
  const field = page.getByTestId("name-input");
  await name.dblclick();
  await expect(field).toBeFocused();
  await expect(field).toHaveValue("2026-11-14_09-05.png");
  // The name is selected, not its extension: typing replaces the name
  await page.keyboard.type("Login bug");
  await expect(field).toHaveValue("Login bug.png");
  await page.keyboard.press("Enter");
  await expect(name).toHaveText("Login bug.png");
  await expect(page.getByTestId("thumb-1")).toBeFocused();

  // Esc puts the name back, and is not the editor's Esc (the tool stays)
  await page.keyboard.press("p");
  await expect(page.getByTestId("tool-pencil")).toHaveAttribute("aria-pressed", "true");
  await name.dblclick();
  await page.keyboard.type("Other");
  await page.keyboard.press("Escape");
  await expect(field).toHaveCount(0);
  await expect(name).toHaveText("Login bug.png");
  await expect(page.getByTestId("tool-pencil")).toHaveAttribute("aria-pressed", "true");

  // Characters a file name cannot hold are refused as typed, with a hint
  await name.dblclick();
  // To the end of the name, before its extension
  await page.keyboard.press("ArrowRight");
  await page.keyboard.type(" v2: a/b?");
  await expect(field).toHaveValue("Login bug v2 ab.png");
  await expect(page.getByTestId("name-hint")).toHaveText('A name cannot contain \\ / : * ? " < > |');
  // Nothing around it moved
  expect(await boxes(page)).toEqual(before);
  await expect(page).toHaveScreenshot("editor-name-editing-light.png");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page).toHaveScreenshot("editor-name-editing-dark.png");
  await page.emulateMedia({ colorScheme: "light" });

  // Trailing dots and spaces go; leaving the field keeps the name
  await page.keyboard.type(". . ");
  await page.getByTestId("statusbar").click();
  await expect(name).toHaveText("Login bug v2 ab.png");
  // Emptied: the previous name comes back
  await name.dblclick();
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Enter");
  await expect(name).toHaveText("Login bug v2 ab.png");

  // Saved under it
  await page.keyboard.press("Control+s");
  await expect.poll(async () => (await exportHeaders(page)).map((headers) => headers["x-name"])).toEqual([
    encodeURIComponent("Login bug v2 ab"),
  ]);
});

test("the image menu: rename, move to front and to end, by pointer and keys", async ({ page }) => {
  await open(page);
  await page.evaluate(() => {
    window.__addImage(1);
    window.__addImage(2);
  });
  await expect(page.getByTestId("thumb-3")).toHaveAttribute("aria-current", "true");
  // The shown image (the third) gets a change of its own
  await drawRectangle(page, [80, 90], [260, 200]);
  expect(await stripNames(page)).toEqual(["2026-11-14_09-05.png", "2026-11-14_09-06.png", "2026-11-14_09-07.png"]);

  // Right-click: the menu, its first item focused; the first image cannot
  // move to the front
  await page.getByTestId("thumb-1").click({ button: "right" });
  const menu = page.getByTestId("menu");
  await expect(menu).toBeVisible();
  await expect(page.getByTestId("menu-rename")).toBeFocused();
  await expect(page.getByTestId("menu-front")).toBeDisabled();
  await expect(page.getByTestId("menu-end")).toBeEnabled();
  await page.waitForTimeout(400);
  await expect(page).toHaveScreenshot("editor-menu-light.png");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page).toHaveScreenshot("editor-menu-dark.png");
  await page.emulateMedia({ colorScheme: "light" });

  // Move to end: the order changes; the shown image stays shown, with its
  // own change and history
  await page.getByTestId("menu-end").click();
  await expect(menu).toHaveCount(0);
  expect(await stripNames(page)).toEqual(["2026-11-14_09-06.png", "2026-11-14_09-07.png", "2026-11-14_09-05.png"]);
  await expect(page.getByTestId("thumb-2")).toHaveAttribute("aria-current", "true");
  await expect(page.getByTestId("position")).toHaveText("Image 2 of 3");
  await expect(page.getByTestId("thumb-2").getByTestId("unsaved")).toHaveCount(1);
  await expect(page.getByTestId("action-undo")).toBeEnabled();

  // By keys: the arrows skip what cannot be picked (Move to end, and the
  // image's own format under Convert to…), Enter picks, and the focus
  // follows the moved image
  await page.getByTestId("thumb-3").click({ button: "right" });
  await expect(page.getByTestId("menu-end")).toBeDisabled();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByTestId("menu-front")).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByTestId("menu-convert-jpg")).toBeFocused();
  await page.keyboard.press("End");
  await expect(page.getByTestId("menu-convert-pdf")).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByTestId("menu-rename")).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(menu).toHaveCount(0);
  expect(await stripNames(page)).toEqual(["2026-11-14_09-05.png", "2026-11-14_09-06.png", "2026-11-14_09-07.png"]);
  await expect(page.getByTestId("thumb-1")).toBeFocused();
  await expect(page.getByTestId("thumb-3")).toHaveAttribute("aria-current", "true");

  // Esc closes it and gives the focus back
  await page.getByTestId("thumb-2").click({ button: "right" });
  await expect(menu).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(page.getByTestId("thumb-2")).toBeFocused();
  // A press elsewhere closes it too
  await page.getByTestId("thumb-2").click({ button: "right" });
  await page.getByTestId("statusbar").click();
  await expect(menu).toHaveCount(0);

  // Rename from the menu
  await page.getByTestId("thumb-2").click({ button: "right" });
  await page.getByTestId("menu-rename").click();
  await expect(page.getByTestId("name-input")).toBeFocused();
  await page.keyboard.type("Second");
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("name-2")).toHaveText("Second.png");

  // Save All numbers them in the strip's order, under their names
  await page.getByTestId("save-all").click();
  await expect.poll(async () => (await exportHeaders(page)).length).toBe(3);
  expect((await exportHeaders(page)).map((headers) => [headers["x-number"], headers["x-name"]])).toEqual([
    ["1", "2026-11-14_09-05"],
    ["2", "Second"],
    ["3", "2026-11-14_09-07"],
  ]);
});

test("Convert to…: the current format is marked, a pick converts, and Save writes it", async ({ page }) => {
  await open(page);
  await page.evaluate(() => window.__addImage(1));
  await page.getByTestId("thumb-1").click({ button: "right" });
  await expect(page.getByRole("group", { name: "Convert to…" })).toBeVisible();
  const png = page.getByTestId("menu-convert-png");
  await expect(png).toBeDisabled();
  await expect(png).toHaveAttribute("aria-checked", "true");
  await expect(page.getByTestId("menu-convert-pdf")).toHaveAttribute("aria-checked", "false");
  await expect(page.getByRole("group", { name: "Convert to…" }).getByRole("menuitemradio")).toHaveText([
    "PNG",
    "JPEG",
    "BMP",
    "WebP",
    "PDF",
  ]);
  await page.getByTestId("menu-convert-pdf").click();
  await expect(page.getByTestId("menu")).toHaveCount(0);
  // Only the format changed: a change to save, no undo step, same pixels
  await expect(page.getByTestId("name-1")).toHaveText("2026-11-14_09-05.pdf");
  await expect(page.getByTestId("thumb-1").getByTestId("unsaved")).toHaveCount(1);
  await expect(page.getByTestId("thumb-2").getByTestId("unsaved")).toHaveCount(0);
  await page.getByTestId("thumb-1").click();
  await expect(page.getByTestId("action-undo")).toBeDisabled();
  // The menu marks the new one
  await page.getByTestId("thumb-1").click({ button: "right" });
  await expect(page.getByTestId("menu-convert-pdf")).toBeDisabled();
  await expect(page.getByTestId("menu-convert-png")).toBeEnabled();
  await page.keyboard.press("Escape");

  // Save and Save All write each image in its own format; Copy keeps the
  // settings' format
  await page.keyboard.press("Control+s");
  await page.keyboard.press("Control+c");
  await page.getByTestId("save-all").click();
  await expect.poll(async () => (await exportHeaders(page)).length).toBe(4);
  expect((await exportHeaders(page)).map((headers) => [headers["x-action"], headers["x-format"]])).toEqual([
    ["save", "pdf"],
    ["copy", undefined],
    ["save_all", "pdf"],
    ["save_all", "png"],
  ]);
  await expect(page.getByTestId("unsaved")).toHaveCount(0);
});

test("rename with an extension: .pdf converts, .tif is refused with a hint", async ({ page }) => {
  await open(page);
  const name = page.getByTestId("name-1");
  const field = page.getByTestId("name-input");
  await name.dblclick();
  await page.keyboard.press("Control+a");
  await page.keyboard.type("Report.PDF");
  await page.keyboard.press("Enter");
  await expect(name).toHaveText("Report.pdf");
  await expect(name).toHaveAttribute("title", "Report.pdf");
  await expect(page.getByTestId("unsaved")).toHaveCount(1);

  // A dot that is not a format stays in the name
  await name.dblclick();
  await page.keyboard.type("v1.2");
  await page.keyboard.press("Enter");
  await expect(name).toHaveText("v1.2.pdf");

  // Another format: refused, the field stays open with the formats named
  const before = await boxes(page);
  await name.dblclick();
  await page.keyboard.press("Control+a");
  await page.keyboard.type("Report.tif");
  await page.keyboard.press("Enter");
  await expect(field).toBeFocused();
  await expect(page.getByTestId("name-hint")).toHaveText("Phramer saves PNG, JPEG, BMP, WebP or PDF");
  expect(await boxes(page)).toEqual(before);
  await expect(page).toHaveScreenshot("editor-format-refused-light.png");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page).toHaveScreenshot("editor-format-refused-dark.png");
  await page.emulateMedia({ colorScheme: "light" });
  // Esc, or leaving the field, keeps the name and format as they were
  await page.keyboard.press("Escape");
  await expect(name).toHaveText("v1.2.pdf");
  await name.dblclick();
  await page.keyboard.press("Control+a");
  await page.keyboard.type("shot.gif");
  await page.getByTestId("statusbar").click();
  await expect(name).toHaveText("v1.2.pdf");

  await page.keyboard.press("Control+s");
  await expect.poll(async () => (await exportHeaders(page)).map((headers) => [headers["x-name"], headers["x-format"]])).toEqual([
    ["v1.2", "pdf"],
  ]);
});

test("Save menu: Show in Folder opens the last saved image, once there is one", async ({ page }) => {
  await open(page);
  const chevron = page.getByTestId("export-as");
  await chevron.click();
  const show = page.getByTestId("menu-show_in_folder");
  await expect(show).toHaveText("Show in Folder");
  await expect(show).toBeDisabled();
  await page.keyboard.press("Escape");

  await page.keyboard.press("Control+s");
  await expect.poll(async () => (await exportHeaders(page)).length).toBe(1);
  await chevron.click();
  await expect(show).toBeEnabled();
  await show.click();
  await expect(page.getByRole("menu", { name: "Export as" })).toHaveCount(0);
  await expect.poll(async () => (await calls(page, "editor_show_last_saved")).length).toBe(1);
});

test("Export as: the list by Save converts the image and is remembered for new images and the next session", async ({ page }) => {
  await open(page);
  // Save keeps its key; the list sits beside it
  await expect(hint(page, "action-save")).toHaveText("(Ctrl+S)");
  const chevron = page.getByTestId("export-as");
  await expect(chevron).toHaveAttribute("aria-expanded", "false");
  await chevron.click();
  const menu = page.getByRole("menu", { name: "Export as" });
  await expect(menu).toBeVisible();
  await expect(chevron).toHaveAttribute("aria-expanded", "true");
  await expect(menu.getByRole("menuitemradio")).toHaveText(["PNG", "JPEG", "BMP", "WebP", "PDF"]);
  await expect(page.getByTestId("menu-png")).toHaveAttribute("aria-checked", "true");
  await expect(page.getByTestId("menu-png")).toBeFocused();
  await expect(page).toHaveScreenshot("editor-export-as-light.png");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page).toHaveScreenshot("editor-export-as-dark.png");
  await page.emulateMedia({ colorScheme: "light" });

  // By keys: WebP
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(menu).toHaveCount(0);
  await expect(chevron).toBeFocused();
  await expect(page.getByTestId("name-1")).toHaveText("2026-11-14_09-05.webp");
  await expect.poll(async () => (await calls(page, "editor_set")).map((args) => JSON.stringify(args))).toContain(
    JSON.stringify({ change: { key: "export_format", format: "webp" } }),
  );
  // Save writes it; Save As too, with the image's format for its dialog
  await page.keyboard.press("Control+s");
  await page.getByTestId("action-save_as").click();
  await expect.poll(async () => (await exportHeaders(page)).length).toBe(2);
  expect((await exportHeaders(page)).map((headers) => [headers["x-action"], headers["x-format"]])).toEqual([
    ["save", "webp"],
    ["save_as", "webp"],
  ]);

  // A new image takes the remembered format
  await page.evaluate(() => window.__addImage(1));
  await expect(page.getByTestId("name-2")).toHaveText("2026-11-14_09-06.webp");
  // A press on the chevron while open closes the list
  await chevron.click();
  await expect(menu).toBeVisible();
  await chevron.click();
  await expect(menu).toHaveCount(0);

  // The next session starts with it (the mock keeps it as the file would)
  await page.reload();
  await expect(page.getByTestId("editor")).toBeVisible();
  await expect(page.getByTestId("name-1")).toHaveText("2026-11-14_09-05.webp");
  await chevron.click();
  await expect(page.getByTestId("menu-webp")).toHaveAttribute("aria-checked", "true");
});
