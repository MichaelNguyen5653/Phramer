// Builds the annotation engine for the editor (crates/phramer-annotate-wasm)
// and copies the module to src/editor/phramer_annotate.wasm, where Vite and
// the tests pick it up. Needs the wasm32-unknown-unknown Rust target:
//   rustup target add wasm32-unknown-unknown
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { withRemappedPaths } from "./remap-paths.mjs";

const app = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(app, "..");
const target = "wasm32-unknown-unknown";

// cargo from CARGO, then PATH, then rustup's default folder
function cargo() {
  if (process.env.CARGO) return process.env.CARGO;
  const found = spawnSync("cargo", ["--version"], { stdio: "ignore", shell: false });
  if (found.status === 0) return "cargo";
  const home = process.env.USERPROFILE || process.env.HOME || "";
  const exe = join(home, ".cargo", "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
  if (existsSync(exe)) return exe;
  console.error("build-wasm: cargo not found; set CARGO or add it to PATH");
  process.exit(1);
}

// Panic messages name source files; keep the build machine's folders (the
// user's profile, the checkout) out of the module that ships
const built = spawnSync(
  cargo(),
  ["build", "-p", "phramer-annotate-wasm", "--target", target, "--release"],
  { cwd: root, stdio: "inherit", env: withRemappedPaths(process.env) },
);
if (built.status !== 0) process.exit(built.status ?? 1);

const targetDir = process.env.CARGO_TARGET_DIR
  ? resolve(root, process.env.CARGO_TARGET_DIR)
  : join(root, "target");
const wasm = join(targetDir, target, "release", "phramer_annotate_wasm.wasm");
copyFileSync(wasm, join(app, "src", "editor", "phramer_annotate.wasm"));
