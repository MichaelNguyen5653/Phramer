/// <reference types="node" />
// The editor's engine is the overlay's: rendered through the wasm module,
// the engine's own test documents give the very checksums the native build
// pins (crates/phramer-annotate/tests/determinism.rs). The fixture is
// written by crates/phramer-annotate-wasm/tests/fixture.rs.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { Edit } from "./engine";
import { loadEngine } from "./testing";

interface Fixture {
  width: number;
  height: number;
  base: string;
  font_family: string;
  font: string;
  workspace: [number, number, number, number];
  documents: { name: string; edits: Edit[]; checksum: string }[];
}

const fixture: Fixture = JSON.parse(
  readFileSync(new URL("../../../crates/phramer-annotate-wasm/tests/fixtures/determinism.json", import.meta.url), "utf8"),
);

const bytes = (base64: string) => new Uint8Array(Buffer.from(base64, "base64"));

/** FNV-1a 64, as determinism.rs computes it. */
function fnv1a(data: Uint8Array): string {
  let hash = 0xcbf29ce484222325n;
  for (const byte of data) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash.toString(16).padStart(16, "0");
}

describe("the wasm engine", () => {
  for (const document of fixture.documents) {
    it(`renders "${document.name}" to the native build's pixels`, async () => {
      const engine = await loadEngine();
      const session = engine.session(fixture.width, fixture.height, bytes(fixture.base), false);
      session.addFont(fixture.font_family, bytes(fixture.font));
      // The pinned checksums are the library's default, with no made-up
      // bold or italic; a session starts with them on, as the overlay draws
      expect(session.call({ op: "set_synthetic_styles", on: false })).toBe(false);
      for (const edit of document.edits) session.apply(edit);
      const [r, g, b, a] = fixture.workspace;
      const image = session.render([], ((r << 24) | (g << 16) | (b << 8) | a) >>> 0);
      const hashed = new Uint8Array(8 + image.rgba.length);
      const view = new DataView(hashed.buffer);
      view.setInt32(0, image.x, true);
      view.setInt32(4, image.y, true);
      hashed.set(image.rgba, 8);
      expect(fnv1a(hashed)).toBe(document.checksum);
      session.free();
    });
  }

  it("refuses requests it does not know and pixels that do not fit", async () => {
    const engine = await loadEngine();
    expect(() => engine.session(2, 2, new Uint8Array(3))).toThrow();
    const session = engine.session(2, 2, new Uint8Array(16));
    expect(() => session.call({ op: "nonsense" })).toThrow();
    expect(session.state()).toEqual({ can_undo: false, can_redo: false, next_counter: 1, objects: 0 });
  });

  it("applies the overlay's Shift rule", async () => {
    const engine = await loadEngine();
    const session = engine.session(2, 2, new Uint8Array(16));
    expect(session.constrain({ x: 0, y: 0 }, { x: 50, y: 3 }, "line")).toEqual({ x: 50, y: 0 });
    expect(session.constrain({ x: 0, y: 0 }, { x: 40, y: 50 }, "rectangle")).toEqual({ x: 45, y: 45 });
  });
});
