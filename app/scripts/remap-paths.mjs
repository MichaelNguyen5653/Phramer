// Panic messages in a release build name source files. These flags map the
// build machine's folders (the cargo home under the user's profile, and the
// checkout) to neutral names, so neither ships in a binary. Shared by
// build-wasm.mjs (the editor's module) and tauri.mjs (`tauri build`).
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const home = process.env.CARGO_HOME || join(process.env.USERPROFILE || process.env.HOME || "", ".cargo");

// CARGO_ENCODED_RUSTFLAGS separates flags with the unit separator, so
// folders with spaces stay whole
const SEPARATOR = String.fromCharCode(0x1f);

/** `env` with the remapping flags added to CARGO_ENCODED_RUSTFLAGS once. */
export function withRemappedPaths(env) {
  const flags = env.CARGO_ENCODED_RUSTFLAGS ? env.CARGO_ENCODED_RUSTFLAGS.split(SEPARATOR) : [];
  for (const flag of [`--remap-path-prefix=${home}=cargo`, `--remap-path-prefix=${root}=phramer`]) {
    if (!flags.includes(flag)) flags.push(flag);
  }
  return { ...env, CARGO_ENCODED_RUSTFLAGS: flags.join(SEPARATOR) };
}
