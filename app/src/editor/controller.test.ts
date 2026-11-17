import { beforeEach, describe, expect, it } from "vitest";
import { EditorController } from "./controller";
import type { Engine, Shape } from "./engine";
import { blank, loadEngine } from "./testing";

const RED = { r: 255, g: 0, b: 0, a: 255 };
const line = (y: number): Shape => ({ tool: "line", start: { x: 10, y }, end: { x: 90, y } });

let engine: Engine;
let editor: EditorController;

beforeEach(async () => {
  engine = await loadEngine();
  editor = EditorController.open(engine, 100, 80, blank(100, 80), 100);
});

function draw(y: number): number {
  editor.startDraft(RED, 3, line(y));
  const id = editor.finishDraft();
  if (id === null) throw new Error("nothing drawn");
  return id;
}

describe("drawing and undo", () => {
  it("keeps a drag as one object and walks the history both ways", () => {
    const a = draw(10);
    draw(20);
    expect(editor.session.state().objects).toBe(2);
    expect(editor.canUndo).toBe(true);
    editor.undo();
    editor.undo();
    expect(editor.session.state().objects).toBe(0);
    expect(editor.canUndo).toBe(false);
    editor.redo();
    expect(editor.session.object(a)?.shape).toEqual(line(10));
    expect(editor.canRedo).toBe(true);
    // A new edit drops what could be redone
    draw(30);
    expect(editor.canRedo).toBe(false);
  });

  it("does not keep a click that drew nothing", () => {
    editor.startDraft(RED, 3, { tool: "line", start: { x: 5, y: 5 }, end: { x: 5, y: 5 } });
    expect(editor.finishDraft()).toBeNull();
    expect(editor.canUndo).toBe(false);
  });

  it("shows previews without recording them", () => {
    const before = editor.render(0xffffffff).rgba.slice();
    editor.startDraft(RED, 3, line(40));
    const during = editor.render(0xffffffff).rgba;
    expect(during).not.toEqual(before);
    expect(editor.session.state().objects).toBe(0);
  });

  it("moves and deletes the selected object as single steps", () => {
    const id = draw(10);
    editor.select(id);
    editor.moveBy(5, 7);
    editor.moveBy(6, 8);
    expect(editor.selectedBounds()?.y).toBeGreaterThan(10);
    editor.finishMove();
    expect(editor.session.object(id)?.shape).toEqual({ tool: "line", start: { x: 16, y: 18 }, end: { x: 96, y: 18 } });
    editor.deleteSelected();
    expect(editor.session.state().objects).toBe(0);
    editor.undo();
    editor.undo();
    expect(editor.session.object(id)?.shape).toEqual(line(10));
  });

  it("follows the undo limit", () => {
    editor = EditorController.open(engine, 100, 80, blank(100, 80), 2);
    draw(10);
    draw(20);
    draw(30);
    editor.undo();
    editor.undo();
    expect(editor.canUndo).toBe(false);
    expect(editor.session.state().objects).toBe(1);
  });
});

describe("live colour", () => {
  it("recolours the selected object while picking, and keeps it as one step", () => {
    const id = draw(10);
    editor.select(id);
    // Pointer moves on the picker: shown at once, nothing recorded yet
    editor.previewColor({ r: 0, g: 10, b: 0 });
    editor.previewColor({ r: 0, g: 200, b: 0 });
    expect(editor.previewEdits()).toContainEqual({ kind: "set_color", id, color: { r: 0, g: 200, b: 0, a: 255 } });
    expect(editor.session.object(id)?.color).toEqual(RED);
    // Released: one step
    editor.commitColor({ r: 255, g: 0, b: 0 }, { r: 0, g: 200, b: 0 });
    expect(editor.session.object(id)?.color).toEqual({ r: 0, g: 200, b: 0, a: 255 });
    const undone = editor.undo();
    expect(undone.color).toEqual({ r: 255, g: 0, b: 0 });
    expect(editor.session.object(id)?.color).toEqual(RED);
    expect(editor.redo().color).toEqual({ r: 0, g: 200, b: 0 });
  });

  it("lets undo take back a pick made with nothing selected", () => {
    draw(10);
    editor.commitColor({ r: 255, g: 0, b: 0 }, { r: 1, g: 2, b: 3 });
    expect(editor.undo().color).toEqual({ r: 255, g: 0, b: 0 });
    // The drawing is still there: the pick was its own step
    expect(editor.session.state().objects).toBe(1);
  });

  it("records nothing when the colour did not change", () => {
    editor.commitColor({ r: 9, g: 9, b: 9 }, { r: 9, g: 9, b: 9 });
    expect(editor.canUndo).toBe(false);
  });

  it("samples the rendered pixels for the eyedropper", () => {
    const image = editor.render(0xffffffff);
    expect(EditorController.sample(image, { x: 3.7, y: 4.2 })).toEqual({ r: 230, g: 230, b: 230 });
    expect(EditorController.sample(image, { x: -1, y: 0 })).toBeNull();
  });
});

describe("text", () => {
  it("types, edits and places text on commit", () => {
    editor.startText({ x: 20, y: 20 }, RED, 8, {}, "left");
    editor.typeText("Hi");
    editor.typeText("!\r\nyo");
    editor.textKey("home");
    editor.textKey("delete");
    expect(editor.text?.text).toBe("Hi!\no");
    expect(editor.caret()?.height).toBeGreaterThan(0);
    editor.commitText();
    expect(editor.text).toBeNull();
    const placed = editor.session.objects();
    expect(placed).toHaveLength(1);
    expect(placed[0].shape).toMatchObject({ tool: "text", text: "Hi!\no" });
  });

  it("places nothing for empty text", () => {
    editor.startText({ x: 20, y: 20 }, RED, 8, {}, "left");
    editor.commitText();
    expect(editor.canUndo).toBe(false);
  });
});

describe("crop", () => {
  it("bakes the annotations into a smaller image, and undo brings the old one back", () => {
    draw(10);
    expect(editor.crop({ x: 10, y: 5, width: 50, height: 20 }, 0xffffffff)).toBe(true);
    expect([editor.session.width, editor.session.height]).toEqual([50, 20]);
    expect(editor.session.state().objects).toBe(0);
    // The line is in the pixels now
    const image = editor.render(0xffffffff);
    expect(EditorController.sample(image, { x: 20, y: 5 })).not.toEqual({ r: 230, g: 230, b: 230 });
    editor.undo();
    expect([editor.session.width, editor.session.height]).toEqual([100, 80]);
    expect(editor.session.state().objects).toBe(1);
    editor.redo();
    expect(editor.session.width).toBe(50);
  });

  it("refuses an empty area", () => {
    expect(editor.crop({ x: 200, y: 200, width: 10, height: 10 }, 0xffffffff)).toBe(false);
  });
});

describe("saved state", () => {
  it("is dirty after an edit and clean once saved", () => {
    expect(editor.dirty).toBe(false);
    draw(10);
    expect(editor.dirty).toBe(true);
    editor.markSaved();
    expect(editor.dirty).toBe(false);
  });
});

describe("text edited again", () => {
  function placeText(text: string): number {
    editor.startText({ x: 10, y: 10 }, RED, 10, { bold: false }, "left");
    editor.typeText(text);
    editor.commitText();
    const placed = editor.session.objects().find((object) => object.shape.tool === "text");
    if (!placed) throw new Error("no text placed");
    return placed.id;
  }

  it("opens a placed text with its words, caret at the end, and changes it as one step", () => {
    const id = placeText("Hello");
    expect(editor.editText(id)).toBe(true);
    expect(editor.text?.text).toBe("Hello");
    expect(editor.text?.caret).toBe(5);
    // Drawn from what is typed, not twice
    expect(editor.previewEdits()[0]).toEqual({ kind: "delete", id });
    editor.typeText(" there");
    editor.text = { ...editor.text!, font: { ...editor.text!.font, bold: true }, size: 14 };
    editor.commitText();
    expect(editor.session.state().objects).toBe(1);
    const changed = editor.session.object(id);
    expect(changed?.shape).toMatchObject({ tool: "text", text: "Hello there", font: { bold: true } });
    expect(changed?.size).toBe(14);
    // One undo takes the whole edit back
    editor.undo();
    expect(editor.session.object(id)?.shape).toMatchObject({ text: "Hello" });
    expect(editor.session.object(id)?.size).toBe(10);
    editor.redo();
    expect(editor.session.object(id)?.shape).toMatchObject({ text: "Hello there" });
  });

  it("records nothing when nothing changed, and removes a text emptied", () => {
    const id = placeText("Hi");
    const before = editor.canRedo;
    editor.undo();
    editor.redo();
    editor.editText(id);
    editor.commitText();
    expect(editor.canRedo).toBe(before);
    editor.undo();
    // The undo took the placing back, not an empty edit
    expect(editor.session.state().objects).toBe(0);
    editor.redo();
    editor.editText(id);
    editor.textKey("backspace");
    editor.textKey("backspace");
    editor.commitText();
    expect(editor.session.object(id)).toBeNull();
  });

  it("is only for text", () => {
    const id = draw(10);
    expect(editor.editText(id)).toBe(false);
    expect(editor.text).toBeNull();
  });
});

describe("disposing", () => {
  it("frees the image and the crops kept for undo", () => {
    draw(10);
    expect(editor.crop({ x: 0, y: 0, width: 50, height: 40 }, 0xffffffff)).toBe(true);
    editor.dispose();
    expect(editor.canUndo).toBe(false);
  });
});
