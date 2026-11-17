// Capture Launcher in Edge with the daemon mocked: reference screenshots,
// the request "Take new screenshot" sends for each mode, the monitor list,
// the countdown, and coming back after an aborted capture.

import { expect, test, type Page } from "@playwright/test";
import { installPanelMock, type PanelMockOptions } from "./panel-mock";

const twoMonitors = [
  { name: "DISPLAY1", width: 1920, height: 1080 },
  { name: "DISPLAY2", width: 2560, height: 1440 },
];

async function openLauncher(page: Page, options: Partial<PanelMockOptions> = {}) {
  await page.addInitScript(installPanelMock, {
    label: "capture-launcher",
    monitors: twoMonitors,
    current: 1,
    lastRegion: { x: 10, y: 20, width: 300, height: 200 },
    ...options,
  });
  await page.goto("/capture-launcher.html");
  await expect(page.getByTestId("capture-launcher")).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.__calls.some((call) => call.cmd === "ui_ready"))).toBe(true);
}

const calls = (page: Page, cmd: string) =>
  page.evaluate((cmd) => window.__calls.filter((call) => call.cmd === cmd).map((call) => call.args), cmd);

test.use({ viewport: { width: 440, height: 330 } });

for (const scheme of ["light", "dark"] as const) {
  test(`the launcher (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await openLauncher(page);
    await expect(page).toHaveScreenshot(`capture-launcher-${scheme}.png`);
  });
}

test("a region capture goes to the chosen monitor with the area", async ({ page }) => {
  await openLauncher(page);
  const monitor = page.getByLabel("Monitor");
  await expect(monitor.locator("option")).toHaveText(["Monitor 1: DISPLAY1 (1920x1080)", "Monitor 2: DISPLAY2 (2560x1440)"]);
  // The monitor under the cursor comes first, the area from the last capture
  await expect(monitor).toHaveValue("1");
  await expect(page.getByLabel("Width")).toHaveValue("300");
  await monitor.selectOption("0");
  await page.getByLabel("X").fill("40");
  const take = page.getByRole("button", { name: "Take new screenshot" });
  await take.click();
  await expect.poll(() => calls(page, "capture_launcher_take")).toEqual([
    { choice: { mode: "region", monitor: 0, region: { x: 40, y: 20, width: 300, height: 200 }, delay_s: 0 } },
  ]);
  await expect(take).toBeDisabled();
});

test("Take new screenshot has the focus, so Enter starts the capture", async ({ page }) => {
  await openLauncher(page);
  await expect(page.getByRole("button", { name: "Take new screenshot" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await calls(page, "capture_launcher_take")).length).toBe(1);
});

test("full screen has no area, and one monitor no list", async ({ page }) => {
  await openLauncher(page, { monitors: [twoMonitors[0]], current: 0, lastRegion: null });
  await expect(page.getByLabel("Monitor")).toHaveCount(0);
  await expect(page.getByLabel("Width")).toHaveValue("0");
  await page.getByLabel("Capture mode").selectOption("full");
  await expect(page.getByLabel("Width")).toHaveCount(0);
  await page.getByRole("button", { name: "Take new screenshot" }).click();
  await expect.poll(() => calls(page, "capture_launcher_take")).toEqual([
    { choice: { mode: "full", monitor: 0, region: { x: 0, y: 0, width: 0, height: 0 }, delay_s: 0 } },
  ]);
});

test("a delay counts down in the window, which then hides itself", async ({ page }) => {
  await page.clock.install();
  await openLauncher(page);
  await expect(page.getByTestId("delay-unit")).toHaveText("No delay");
  await page.getByLabel("Delay").fill("2");
  await expect(page.getByTestId("delay-unit")).toHaveText("seconds");
  await page.getByRole("button", { name: "Take new screenshot" }).click();
  const countdown = page.getByTestId("countdown");
  await expect(countdown).toHaveText("2");
  expect(await calls(page, "plugin:window|hide")).toEqual([]);
  await page.clock.runFor(1000);
  await expect(countdown).toHaveText("1");
  await page.clock.runFor(1000);
  await expect(countdown).toHaveCount(0);
  await expect.poll(async () => (await calls(page, "plugin:window|hide")).length).toBe(1);
});

test("an aborted capture brings it back ready for another try", async ({ page }) => {
  await openLauncher(page);
  const take = page.getByRole("button", { name: "Take new screenshot" });
  await take.click();
  await expect(take).toBeDisabled();
  await page.evaluate(() => window.__emit("capture-launcher:back"));
  await expect(take).toBeEnabled();
});

test("Esc closes it", async ({ page }) => {
  await openLauncher(page);
  await page.keyboard.press("Escape");
  await expect.poll(async () => (await calls(page, "plugin:window|close")).length).toBe(1);
});
