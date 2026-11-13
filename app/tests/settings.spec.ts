// The settings and welcome pages in Edge with the daemon mocked: reference
// screenshots of every section (light and dark), search, the shortcut
// recorder and the welcome steps; changes that apply with no Apply or OK;
// reduced motion; no layout shift.

import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";
import { installSettingsMock } from "./settings-mock";

const snapshot = JSON.parse(readFileSync(new URL("./fixtures/settings-default.json", import.meta.url), "utf8"));
const SECTIONS = ["general", "capture", "saving", "appearance", "editing", "shortcuts", "advanced"];

async function watchShifts(page: Page) {
  await page.addInitScript(() => {
    (window as unknown as { __shift: number }).__shift = 0;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries() as (PerformanceEntry & { value: number; hadRecentInput: boolean })[]) {
        if (!entry.hadRecentInput) (window as unknown as { __shift: number }).__shift += entry.value;
      }
    }).observe({ type: "layout-shift", buffered: true });
  });
}

async function openSettings(page: Page) {
  await page.addInitScript(installSettingsMock, { snapshot, label: "settings" });
  await watchShifts(page);
  await page.goto("/settings.html");
  await expect(page.getByTestId("settings")).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.__calls.some((call) => call.cmd === "ui_ready"))).toBe(true);
}

const calls = (page: Page, cmd: string) =>
  page.evaluate((cmd) => window.__calls.filter((call) => call.cmd === cmd).map((call) => call.args), cmd);

const shift = (page: Page) => page.evaluate(() => (window as unknown as { __shift: number }).__shift);

test.describe("settings", () => {
  test.use({ viewport: { width: 880, height: 620 } });

  for (const scheme of ["light", "dark"] as const) {
    test(`every section, ${scheme}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await openSettings(page);
      for (const section of SECTIONS) {
        await page.getByTestId(`nav-${section}`).click();
        await expect(page.getByTestId(`section-${section}`)).toBeVisible();
        await expect(page.getByTestId(`nav-${section}`)).toHaveAttribute("aria-current", "page");
        if (section === "saving") await expect(page.getByTestId("pattern-preview")).toContainText("2026-11-13_09-41.png");
        await expect(page).toHaveScreenshot(`settings-${section}-${scheme}.png`);
      }
      // Nothing waits for an OK or Apply
      await expect(page.getByRole("button", { name: /^(ok|apply|save|done|confirm)$/i })).toHaveCount(0);
      expect(await shift(page)).toBeLessThan(0.01);
    });
  }

  test("a switch applies at once and resets to its default", async ({ page }) => {
    await openSettings(page);
    await page.getByTestId("nav-capture").click();
    const row = page.getByTestId("row-showHelp");
    const toggle = row.getByRole("switch");
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await expect(page.getByTestId("reset-showHelp")).toHaveClass(/hidden/);

    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await expect.poll(() => calls(page, "settings_set")).toEqual([{ key: "showHelp", value: false }]);
    await expect(page.getByTestId("reset-showHelp")).not.toHaveClass(/hidden/);

    await page.getByTestId("reset-showHelp").click();
    await expect.poll(() => calls(page, "settings_reset")).toEqual([{ key: "showHelp" }]);
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    // The inverted one: "Show the tray icon" writes disabledTrayIcon
    await page.getByTestId("nav-general").click();
    await page.getByTestId("row-disabledTrayIcon").getByRole("switch").click();
    await expect.poll(async () => (await calls(page, "settings_set")).at(-1)).toEqual({ key: "disabledTrayIcon", value: true });
  });

  test("numbers out of range are refused before they are written", async ({ page }) => {
    await openSettings(page);
    await page.getByTestId("nav-editing").click();
    const field = page.getByTestId("row-undoLimit").getByRole("textbox");
    await field.fill("1000");
    await expect(page.getByTestId("row-undoLimit").getByRole("alert")).toHaveText("Enter 1 to 999");
    await field.fill("250");
    await expect(page.getByTestId("row-undoLimit").getByRole("alert")).toHaveCount(0);
    await expect.poll(async () => (await calls(page, "settings_set")).at(-1)).toEqual({ key: "undoLimit", value: 250 });
    expect((await calls(page, "settings_set")).some((args) => (args as { value: number }).value === 1000)).toBe(false);
  });

  test("search finds settings across sections, typos included", async ({ page }) => {
    await openSettings(page);
    await page.getByTestId("search").fill("magnifer");
    const results = page.getByTestId("search-results");
    await expect(results.getByTestId("row-showMagnifier")).toBeVisible();
    await expect(results.getByTestId("row-squareMagnifier")).toBeVisible();
    await expect(page).toHaveScreenshot("settings-search.png");
    await page.getByTestId("search").fill("pencil");
    await expect(results.getByTestId("row-shortcut-TYPE_PENCIL")).toBeVisible();
    await page.getByTestId("search").fill("zzqqx");
    await expect(results).toContainText("No settings match");
    // Esc clears the search first, and only then would close the window
    await page.getByTestId("search").press("Escape");
    await expect(page.getByTestId("section-general")).toBeVisible();
    expect(await calls(page, "plugin:window|close")).toEqual([]);
    await page.keyboard.press("Control+f");
    await expect(page.getByTestId("search")).toBeFocused();
  });

  test("the shortcut recorder saves keys at once and reports conflicts", async ({ page }) => {
    await openSettings(page);
    await page.getByTestId("nav-shortcuts").click();
    const pencil = page.getByTestId("row-shortcut-TYPE_PENCIL").getByRole("button", { name: /shortcut for/ });
    await pencil.click();
    await expect.poll(() => calls(page, "settings_pause_hotkeys")).toEqual([{ paused: true }]);
    await page.keyboard.down("Control");
    await page.keyboard.down("Shift");
    await expect(page.getByTestId("row-shortcut-TYPE_PENCIL")).toHaveScreenshot("settings-recorder.png");
    await page.keyboard.press("KeyK");
    await page.keyboard.up("Shift");
    await page.keyboard.up("Control");
    await expect.poll(() => calls(page, "settings_set_shortcut")).toEqual([{ name: "TYPE_PENCIL", sequence: "Ctrl+Shift+K" }]);
    await expect.poll(async () => (await calls(page, "settings_pause_hotkeys")).at(-1)).toEqual({ paused: false });
    await expect(page.getByTestId("row-shortcut-TYPE_PENCIL").locator("kbd")).toHaveText(["Ctrl", "Shift", "K"]);

    // G is the move tool's default: the pencil takes it and says so
    await pencil.click();
    await page.keyboard.press("KeyG");
    await expect(page.getByTestId("row-shortcut-TYPE_PENCIL").getByRole("status")).toContainText("Taken from");
    // Now the pencil's own: the arrow cannot have it
    const arrow = page.getByTestId("row-shortcut-TYPE_ARROW").getByRole("button", { name: /shortcut for/ });
    await arrow.click();
    await page.keyboard.press("KeyG");
    await expect(page.getByTestId("row-shortcut-TYPE_ARROW").getByRole("alert")).toContainText("already used by");
    await expect(page.getByTestId("row-shortcut-TYPE_ARROW").locator("kbd")).toHaveText(["A"]);

    // A bare key cannot be a global hotkey; Esc cancels without closing
    const capture = page.getByTestId("row-shortcut-TAKE_SCREENSHOT").getByRole("button", { name: /shortcut for/ });
    await capture.click();
    await page.keyboard.press("KeyQ");
    await expect(page.getByTestId("row-shortcut-TAKE_SCREENSHOT").getByRole("alert")).toContainText("Add Ctrl");
    await page.keyboard.press("Escape");
    await expect(capture).toHaveAttribute("aria-pressed", "false");
    expect(await calls(page, "plugin:window|close")).toEqual([]);
    await expect(page.getByTestId("row-shortcut-TAKE_SCREENSHOT").locator("kbd")).toHaveText(["Win", "Shift", "X"]);
  });

  test("Print Screen can be handed to Phramer and back", async ({ page }) => {
    await openSettings(page);
    await page.getByTestId("nav-shortcuts").click();
    const banner = page.getByTestId("print-screen-banner");
    await expect(banner).toBeVisible();
    await banner.getByRole("button", { name: "Use Print Screen" }).click();
    await expect.poll(() => calls(page, "settings_print_screen")).toEqual([{ phramer: true }]);
    await expect(banner).toHaveCount(0);
    const toggle = page.getByTestId("row-printScreen").getByRole("switch");
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await toggle.click();
    await expect.poll(async () => (await calls(page, "settings_print_screen")).at(-1)).toEqual({ phramer: false });
    await expect(toggle).toHaveAttribute("aria-checked", "false");
  });

  test("the file name pattern previews as it is typed", async ({ page }) => {
    await openSettings(page);
    await page.getByTestId("nav-saving").click();
    const input = page.getByTestId("pattern-input");
    await input.fill("shot_");
    await page.getByRole("button", { name: "Year (2000)" }).click();
    await expect(input).toHaveValue("shot_%Y");
    await expect(page.getByTestId("pattern-preview")).toContainText("shot_2026.png");
    await expect.poll(async () => (await calls(page, "settings_set")).at(-1)).toEqual({ key: "filenamePattern", value: "shot_%Y" });
    await input.fill("bad*name");
    await expect(page.getByTestId("row-filenamePattern").getByRole("alert")).toContainText("does not allow *");
    expect((await calls(page, "settings_set")).some((args) => (args as { value: string }).value === "bad*name")).toBe(false);
  });

  test("palette colours change live and are saved when the gesture ends", async ({ page }) => {
    await openSettings(page);
    await page.getByTestId("nav-appearance").click();
    await page.getByTestId("palette-4").click();
    const picker = page.getByTestId("color-picker");
    await expect(picker).toBeVisible();
    await expect(picker.getByRole("button", { name: /^(ok|apply|done|confirm)$/i })).toHaveCount(0);
    const square = await page.getByTestId("picker-square").boundingBox();
    if (!square) throw new Error("no square");
    await page.mouse.move(square.x + 20, square.y + 20);
    await page.mouse.down();
    await page.mouse.move(square.x + square.width - 10, square.y + 10, { steps: 3 });
    expect(await calls(page, "settings_set")).toEqual([]);
    await page.mouse.up();
    await expect.poll(async () => (await calls(page, "settings_set")).length).toBe(1);
    const [saved] = (await calls(page, "settings_set")) as { key: string; value: (string | null)[] }[];
    expect(saved.key).toBe("userColors");
    expect(saved.value[4]).not.toBe("#ff0000");
    expect(saved.value).toHaveLength(12);
    await page.keyboard.press("Escape");
    await expect(picker).toHaveCount(0);
  });

  test("reduced motion makes every change instant", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await openSettings(page);
    const durations = await page.evaluate(() => {
      const thumb = document.querySelector(".switch-thumb") as HTMLElement;
      const section = document.querySelector(".section") as HTMLElement;
      return [getComputedStyle(thumb).transitionDuration, getComputedStyle(section).animationDuration];
    });
    expect(durations).toEqual(["0s", "0s"]);
  });

  test("Esc closes the window", async ({ page }) => {
    await openSettings(page);
    await page.keyboard.press("Escape");
    await expect.poll(() => calls(page, "plugin:window|close")).toHaveLength(1);
  });
});

test.describe("welcome", () => {
  test.use({ viewport: { width: 600, height: 440 } });

  async function openWelcome(page: Page, mandatory: boolean) {
    await page.addInitScript(installSettingsMock, { snapshot, label: "welcome", mandatory });
    await watchShifts(page);
    await page.goto("/welcome.html");
    await expect(page.getByTestId("welcome")).toBeVisible();
    await expect.poll(() => page.evaluate(() => window.__calls.some((call) => call.cmd === "ui_ready"))).toBe(true);
  }

  test("three steps, light, with choices applied live", async ({ page }) => {
    await openWelcome(page, false);
    await expect(page.getByTestId("welcome-step-0").locator("kbd")).toHaveText(["Win", "Shift", "X"]);
    await expect(page).toHaveScreenshot("welcome-step-1.png");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("welcome-step-1")).toBeVisible();
    await expect(page).toHaveScreenshot("welcome-step-2.png");

    await page.getByTestId("welcome-print-screen").getByRole("switch").click();
    await expect.poll(() => calls(page, "settings_print_screen")).toEqual([{ phramer: true }]);
    await page.getByTestId("welcome-startup").getByRole("switch").click();
    await expect.poll(() => calls(page, "settings_set")).toEqual([{ key: "startupLaunch", value: false }]);
    await expect(page.getByTestId("welcome-startup").getByRole("switch")).toHaveAttribute("aria-checked", "false");

    await page.getByTestId("welcome-next").click();
    await expect(page.getByTestId("welcome-step-2")).toBeVisible();
    await expect(page).toHaveScreenshot("welcome-step-3.png");
    await page.getByTestId("welcome-settings").click();
    await expect.poll(() => calls(page, "settings_open")).toHaveLength(1);
    expect(await shift(page)).toBeLessThan(0.01);
  });

  test("dark", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    await openWelcome(page, false);
    await expect(page).toHaveScreenshot("welcome-step-1-dark.png");
  });

  test("a new major version is seen to its end before it closes", async ({ page }) => {
    await openWelcome(page, true);
    await expect(page.getByRole("button", { name: "Skip" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Close" })).toHaveCount(0);
    await page.keyboard.press("Escape");
    expect(await calls(page, "plugin:window|close")).toEqual([]);
    expect(await calls(page, "welcome_reached_end")).toEqual([]);
    await page.keyboard.press("Enter");
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("welcome-step-2")).toBeVisible();
    await expect.poll(() => calls(page, "welcome_reached_end")).toHaveLength(1);
    await page.keyboard.press("Escape");
    await expect.poll(() => calls(page, "plugin:window|close")).toHaveLength(1);
  });
});
