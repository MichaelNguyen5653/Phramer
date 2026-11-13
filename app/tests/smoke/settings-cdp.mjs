// Used only by the live smoke test `settings_smoke.rs`: drives the open
// settings window of a running daemon through WebView2's DevTools port, the
// way a user would click it, without sending any real input to Windows.
//
//   node tests/smoke/settings-cdp.mjs <port> toggle-tray
//
// Prints the tray switch's state after the click.

import { chromium } from "@playwright/test";

const [port, action] = process.argv.slice(2);
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
const page = browser
  .contexts()
  .flatMap((context) => context.pages())
  .find((candidate) => candidate.url().includes("settings.html"));
if (!page) {
  console.log("no settings page");
  process.exit(2);
}

if (action === "toggle-tray") {
  await page.getByTestId("nav-general").click();
  const toggle = page.getByTestId("row-disabledTrayIcon").getByRole("switch");
  const before = await toggle.getAttribute("aria-checked");
  await toggle.click();
  await page.waitForFunction(
    ([element, old]) => element?.getAttribute("aria-checked") !== old,
    [await toggle.elementHandle(), before],
  );
  console.log(`tray switch: ${await toggle.getAttribute("aria-checked")}`);
} else {
  console.log(`unknown action ${action}`);
  process.exit(2);
}
// Leaves the window and the browser running; only the connection ends
process.exit(0);
