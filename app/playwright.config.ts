import { defineConfig } from "@playwright/test";

// Browser tests of the editor page against the Vite dev server, with the
// Tauri API mocked (tests/tauri-mock.ts). Edge is what WebView2 is built
// on and ships with Windows, so no browser is downloaded.
// Reference screenshots live in tests/screenshots; `--update-snapshots`
// rewrites them.
const port = 5179;

export default defineConfig({
  testDir: "tests",
  snapshotPathTemplate: "tests/screenshots/{arg}{ext}",
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  expect: {
    toHaveScreenshot: { maxDiffPixelRatio: 0.002, animations: "disabled", caret: "hide" },
  },
  use: {
    baseURL: `http://localhost:${port}`,
    channel: "msedge",
    viewport: { width: 1200, height: 760 },
    deviceScaleFactor: 1,
    colorScheme: "light",
    reducedMotion: "no-preference",
  },
  webServer: {
    command: `npx vite --port ${port} --strictPort`,
    url: `http://localhost:${port}/editor.html`,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
