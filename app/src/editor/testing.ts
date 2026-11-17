/// <reference types="node" />
// Loads the built engine module for tests that run in Node.
import { readFileSync } from "node:fs";
import { Engine } from "./engine";

let loaded: Promise<Engine> | null = null;

export function loadEngine(): Promise<Engine> {
  loaded ??= Engine.fromBytes(readFileSync(new URL("./phramer_annotate.wasm", import.meta.url)));
  return loaded;
}

/** A plain light grey image. */
export function blank(width: number, height: number): Uint8Array {
  return new Uint8Array(width * height * 4).fill(230);
}
