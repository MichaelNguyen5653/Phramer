// App Launcher in Edge with the daemon mocked: reference screenshots
// (light, dark, loading), type-to-filter, the keyboard, choosing an app with
// and without "Keep open after selection", the "Open with" fallback,
// failures and Esc.

import { expect, test, type Page } from "@playwright/test";
import { installPanelMock, type PanelMockOptions } from "./panel-mock";

const apps = [
  { name: "GIMP 2.10", exe: "C:\\Program Files\\GIMP 2\\bin\\gimp-2.10.exe", graphics: true },
  { name: "Paint.NET", exe: "C:\\Program Files\\paint.net\\paintdotnet.exe", graphics: true },
  { name: "Microsoft Edge", exe: "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe", graphics: false },
  { name: "Notepad", exe: "C:\\Windows\\System32\\notepad.exe", graphics: false },
  { name: "Notepad++", exe: "C:\\Program Files\\Notepad++\\notepad++.exe", graphics: false },
  { name: "Remote Desktop Connection", exe: "C:\\Windows\\system32\\mstsc.exe", graphics: false },
  { name: "Visual Studio Code", exe: "C:\\Users\\you\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe", graphics: false },
];
const icons = {
  [apps[0].exe]: "#6b5b45",
  [apps[1].exe]: "#2f6fd0",
  [apps[2].exe]: "#1d9a8a",
  [apps[3].exe]: "#4a8fd8",
  [apps[6].exe]: "#2a7ac7",
};

async function openLauncher(page: Page, options: Partial<PanelMockOptions> = {}) {
  await page.addInitScript(installPanelMock, { label: "launcher-1", apps, icons, ...options });
  await page.goto("/launcher.html");
  await expect(page.getByTestId("launcher")).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.__calls.some((call) => call.cmd === "ui_ready"))).toBe(true);
}

const calls = (page: Page, cmd: string) =>
  page.evaluate((cmd) => window.__calls.filter((call) => call.cmd === cmd).map((call) => call.args), cmd);
const names = (page: Page) => page.getByTestId("launcher-app").locator(".launcher-name").allTextContents();
const selected = (page: Page) => page.locator('[role="option"][aria-selected="true"] .launcher-name').textContent();

test.use({ viewport: { width: 560, height: 440 } });

for (const scheme of ["light", "dark"] as const) {
  test(`the app list (${scheme})`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await openLauncher(page);
    await expect(page.getByTestId("launcher-app")).toHaveCount(apps.length);
    await expect(page.locator("img.launcher-icon")).toHaveCount(Object.keys(icons).length);
    await expect(page.getByTestId("launcher-search")).toBeFocused();
    await expect(page.locator(".launcher-section")).toHaveText(["Graphics", "Utility", "Other"]);
    await expect(page).toHaveScreenshot(`launcher-list-${scheme}.png`);
  });
}

test("the list shows at once and fills in when read", async ({ page }) => {
  await openLauncher(page, { delay: 1500 });
  await expect(page.getByRole("status")).toHaveText("Reading the Start Menu\u2026");
  // The fallback is there before the list
  await expect(page.getByTestId("launcher-open-with")).toBeVisible();
  await expect(page).toHaveScreenshot("launcher-loading.png");
  await expect(page.getByTestId("launcher-app")).toHaveCount(apps.length);
});

test("typing filters, the arrows move and Enter opens the capture, then the window closes", async ({ page }) => {
  await openLauncher(page);
  await expect(page.getByTestId("launcher-app")).toHaveCount(apps.length);
  await page.keyboard.type("pad");
  await expect.poll(() => names(page)).toEqual(["Notepad", "Notepad++"]);
  expect(await selected(page)).toBe("Notepad");
  await page.keyboard.press("ArrowDown");
  expect(await selected(page)).toBe("Notepad++");
  await page.keyboard.press("Enter");
  await expect.poll(() => calls(page, "launcher_launch")).toEqual([{ exe: apps[4].exe }]);
  await expect.poll(() => calls(page, "plugin:window|close")).toHaveLength(1);
});

test("wildcards match as in 15.0.0", async ({ page }) => {
  await openLauncher(page);
  await page.getByTestId("launcher-search").fill("v*code");
  await expect.poll(() => names(page)).toEqual(["Visual Studio Code"]);
  await page.getByTestId("launcher-search").fill("zzz");
  await expect(page.getByText("No apps match \u201czzz\u201d")).toBeVisible();
  await expect(page.getByTestId("launcher-open-with")).toBeVisible();
});

test("with Keep open on, the window stays and says where it went", async ({ page }) => {
  await openLauncher(page, { keepOpen: true });
  await expect(page.getByTestId("launcher-keep-open").getByRole("switch")).toHaveAttribute("aria-checked", "true");
  await page.getByTestId("launcher-app").filter({ hasText: "Paint.NET" }).click();
  await expect(page.getByTestId("launcher-message")).toHaveText("Opened in Paint.NET.");
  expect(await calls(page, "launcher_launch")).toEqual([{ exe: apps[1].exe }]);
  expect(await calls(page, "plugin:window|close")).toEqual([]);
  // Focus stays in the search field
  await expect(page.getByTestId("launcher-search")).toBeFocused();
});

test("the Keep open switch is remembered", async ({ page }) => {
  await openLauncher(page);
  await page.getByTestId("launcher-keep-open").getByRole("switch").click();
  await expect(page.getByTestId("launcher-keep-open").getByRole("switch")).toHaveAttribute("aria-checked", "true");
  expect(await calls(page, "launcher_set_keep_open")).toEqual([{ keepOpen: true }]);
});

test("Open with... is the last entry, one up from the top", async ({ page }) => {
  await openLauncher(page);
  await expect(page.getByTestId("launcher-app")).toHaveCount(apps.length);
  await page.keyboard.press("ArrowUp");
  await expect(page.getByTestId("launcher-open-with")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Enter");
  await expect.poll(() => calls(page, "launcher_open_with")).toHaveLength(1);
  await expect.poll(() => calls(page, "plugin:window|close")).toHaveLength(1);
});

test("a program that fails to start is reported and the window stays", async ({ page }) => {
  await openLauncher(page, { launchError: "Unable to launch C:\\x.exe: The system cannot find the file specified." });
  await expect(page.getByTestId("launcher-app")).toHaveCount(apps.length);
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("launcher-message")).toHaveText(
    "Unable to launch C:\\x.exe: The system cannot find the file specified.",
  );
  expect(await calls(page, "plugin:window|close")).toEqual([]);
});

test("Esc closes the window", async ({ page }) => {
  await openLauncher(page);
  await page.keyboard.press("Escape");
  await expect.poll(() => calls(page, "plugin:window|close")).toHaveLength(1);
});
