// Used only by the live smoke test `ocr_launcher_smoke.rs`: drives the OCR
// results window and App Launcher of a running daemon through WebView2's
// DevTools port, without sending any real input to Windows.
//
//   node tests/smoke/panels-cdp.mjs <port> <action> [argument]
//
// Actions (each prints one JSON line):
//   ocr-result            waits for the first result: {text, status, hint, diagnostics}
//   ocr-harder            clicks Try harder, waits for its result: as above
//   launcher-list         waits for the app list: {names}
//   launcher-pick <name>  types <name>, waits for it to be selected, presses Enter
//   pages                 every page: {pages: [{url, ready, text}]}, for a failure report

import { chromium } from "@playwright/test";

const [port, action, argument] = process.argv.slice(2);
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);

async function pageOf(file) {
  for (let attempt = 0; attempt < 100; attempt++) {
    const page = browser
      .contexts()
      .flatMap((context) => context.pages())
      .find((candidate) => candidate.url().includes(file));
    if (page) return page;
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  console.log(JSON.stringify({ error: `no ${file} page` }));
  process.exit(2);
}

async function ocrState(page) {
  return page.evaluate(() => ({
    text: document.querySelector('[data-testid="ocr-text"]')?.value ?? "",
    status: document.querySelector('[data-testid="ocr-status"]')?.textContent ?? "",
    hint: document.querySelector('[data-testid="ocr-hint"]')?.textContent ?? "",
    diagnostics: document.querySelector('[data-testid="ocr-hint"]')?.getAttribute("title") ?? "",
  }));
}

/** A result is in: Try harder or Copy all became usable, or a failure shows. */
async function waitForResult(page) {
  await page.waitForFunction(
    () => {
      const harder = document.querySelector('[data-testid="ocr-try-harder"]');
      const status = document.querySelector('[data-testid="ocr-status"]');
      const hint = document.querySelector('[data-testid="ocr-hint"]');
      return (hint?.getAttribute("title") ?? "") !== "" || (status && status.getAttribute("role") === "alert") || (harder && !harder.disabled);
    },
    null,
    { timeout: 120_000 },
  );
}

if (action === "pages") {
  const pages = [];
  for (const page of browser.contexts().flatMap((context) => context.pages())) {
    pages.push({
      url: page.url(),
      ...(await page
        .evaluate(() => ({ ready: document.readyState, text: document.body?.innerText.slice(0, 300) ?? "" }))
        .catch((error) => ({ error: String(error) }))),
    });
  }
  console.log(JSON.stringify({ pages }));
} else if (action === "ocr-result") {
  const page = await pageOf("ocr.html");
  await waitForResult(page);
  console.log(JSON.stringify(await ocrState(page)));
} else if (action === "ocr-harder") {
  const page = await pageOf("ocr.html");
  await waitForResult(page);
  await page.getByTestId("ocr-try-harder").click();
  // The click clears the result; wait for the new one
  await page.waitForFunction(() => (document.querySelector('[data-testid="ocr-hint"]')?.getAttribute("title") ?? "") === "", null, {
    timeout: 5_000,
  }).catch(() => {});
  await waitForResult(page);
  console.log(JSON.stringify(await ocrState(page)));
} else if (action === "launcher-list") {
  const page = await pageOf("launcher.html");
  await page.getByTestId("launcher-app").first().waitFor({ timeout: 120_000 });
  const names = await page.getByTestId("launcher-app").locator(".launcher-name").allTextContents();
  console.log(JSON.stringify({ names }));
} else if (action === "launcher-pick") {
  const page = await pageOf("launcher.html");
  await page.getByTestId("launcher-app").first().waitFor({ timeout: 120_000 });
  await page.getByTestId("launcher-search").fill(argument);
  await page.waitForFunction(
    (name) => document.querySelector('[role="option"][aria-selected="true"] .launcher-name')?.textContent === name,
    argument,
    { timeout: 10_000 },
  );
  await page.getByTestId("launcher-search").press("Enter");
  console.log(JSON.stringify({ picked: argument }));
} else {
  console.log(JSON.stringify({ error: `unknown action ${action}` }));
  process.exit(2);
}
// Leaves the windows and the browser running; only the connection ends
process.exit(0);
