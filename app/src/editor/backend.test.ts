import { describe, expect, it } from "vitest";
import { parseFaces } from "./backend";
import { emptyHint } from "./Empty";

describe("font faces from the daemon", () => {
  it("read each face's style, collection index and bytes", () => {
    const faces = [
      { bold: 0, italic: 0, index: 0, data: [1, 2, 3] },
      { bold: 1, italic: 1, index: 2, data: [9] },
    ];
    const size = 4 + faces.reduce((sum, face) => sum + 16 + face.data.length, 0);
    const buffer = new ArrayBuffer(size);
    const view = new DataView(buffer);
    view.setUint32(0, faces.length, true);
    let at = 4;
    for (const face of faces) {
      view.setUint32(at, face.bold, true);
      view.setUint32(at + 4, face.italic, true);
      view.setUint32(at + 8, face.index, true);
      view.setUint32(at + 12, face.data.length, true);
      new Uint8Array(buffer, at + 16, face.data.length).set(face.data);
      at += 16 + face.data.length;
    }
    const read = parseFaces("Cambria", buffer);
    expect(read.map(({ family, bold, italic, index }) => ({ family, bold, italic, index }))).toEqual([
      { family: "Cambria", bold: false, italic: false, index: 0 },
      { family: "Cambria", bold: true, italic: true, index: 2 },
    ]);
    expect([...read[0].data]).toEqual([1, 2, 3]);
    expect([...read[1].data]).toEqual([9]);
    expect(parseFaces("None", new ArrayBuffer(0))).toEqual([]);
  });
});

describe("the empty editor", () => {
  const state = { capture_hotkey: "Meta+Shift+x", print_screen: false, auto_open_in_editor: false, editor_key: "E" };

  it("says how to capture into it, as 15.0.0 did", () => {
    expect(emptyHint(state)).toBe("Press Shift+Win+X to capture. Then pick Editor (E) on the capture toolbar.");
    expect(emptyHint({ ...state, print_screen: true })).toBe(
      "Press Shift+Win+X or Print Screen to capture. Then pick Editor (E) on the capture toolbar.",
    );
    expect(emptyHint({ ...state, capture_hotkey: "", editor_key: "" })).toBe(
      "Take a capture from the tray icon. Then pick Editor on the capture toolbar.",
    );
    expect(emptyHint({ ...state, auto_open_in_editor: true })).toBe("Press Shift+Win+X to capture. It opens here automatically.");
    // Print Screen as the capture key is named once
    expect(emptyHint({ ...state, capture_hotkey: "Print", print_screen: true })).toMatch(/^Press Print Screen to capture\./);
  });
});
