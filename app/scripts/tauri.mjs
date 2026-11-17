// `npm run tauri`: the Tauri CLI. `tauri build` gets the path remapping of
// remap-paths.mjs, so the release binaries name no folder of the build
// machine; other commands run unchanged, sharing the build cache with cargo.
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { withRemappedPaths } from "./remap-paths.mjs";

const app = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const env = args[0] === "build" ? withRemappedPaths(process.env) : process.env;
const ran = spawnSync(process.execPath, [join(app, "node_modules", "@tauri-apps", "cli", "tauri.js"), ...args], {
  stdio: "inherit",
  env,
});
if (ran.error) {
  console.error(`tauri: ${ran.error.message}`);
  process.exit(1);
}
process.exit(ran.status ?? 1);
