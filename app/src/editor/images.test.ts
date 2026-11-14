import { beforeEach, describe, expect, it } from "vitest";
import { EditorController } from "./controller";
import type { Engine, Shape } from "./engine";
import { closeQuestion, ImageSet, positionLabel, removeQuestion } from "./images";
import { blank, loadEngine } from "./testing";

const RED = { r: 255, g: 0, b: 0, a: 255 };
const line: Shape = { tool: "line", start: { x: 10, y: 10 }, end: { x: 90, y: 10 } };

let engine: Engine;
const open = (width = 100) => EditorController.open(engine, width, 80, blank(width, 80), 100);

function draw(editor: EditorController): void {
  editor.startDraft(RED, 3, line);
  editor.finishDraft();
}

beforeEach(async () => {
  engine = await loadEngine();
});

describe("the images of one window", () => {
  it("adds an image at the end and shows it", () => {
    const set = new ImageSet(open());
    const second = set.add(open(120));
    const third = set.add(open(140));
    expect(set.count).toBe(3);
    expect(set.position).toBe(2);
    expect(set.current).toBe(third);
    expect(set.list.map((page) => page.doc.session.width)).toEqual([100, 120, 140]);
    expect(new Set(set.list.map((page) => page.id)).size).toBe(3);
    expect(second.doc).not.toBe(third.doc);
  });

  it("keeps each image's own undo history and unsaved state", () => {
    const set = new ImageSet(open());
    set.add(open());
    draw(set.current.doc);
    expect(set.current.doc.canUndo).toBe(true);
    set.show(0);
    expect(set.current.doc.canUndo).toBe(false);
    expect(set.current.doc.dirty).toBe(false);
    expect(set.unsaved()).toEqual([2]);
    set.show(1);
    set.current.doc.undo();
    expect(set.current.doc.session.state().objects).toBe(0);
    // Undoing still counts as a change since the last save
    expect(set.unsaved()).toEqual([2]);
  });

  it("places typed text when another image is shown", () => {
    const set = new ImageSet(open());
    set.add(open());
    const editor = set.current.doc;
    editor.startText({ x: 10, y: 10 }, RED, 10, {}, "left");
    editor.typeText("Hi");
    set.show(0);
    expect(editor.text).toBeNull();
    expect(editor.session.state().objects).toBe(1);
  });

  it("removes an image, shows its neighbour and never removes the last one", () => {
    const set = new ImageSet(open(100));
    set.add(open(120));
    set.add(open(140));
    set.show(1);
    expect(set.remove()).toBe(true);
    expect(set.list.map((page) => page.doc.session.width)).toEqual([100, 140]);
    // The one after it takes its place
    expect(set.current.doc.session.width).toBe(140);
    // The last of the list: the new last one shows
    expect(set.remove()).toBe(true);
    expect(set.current.doc.session.width).toBe(100);
    // Removing one before the shown image keeps the same image shown
    set.add(open(160));
    set.remove(0);
    expect(set.current.doc.session.width).toBe(160);
    expect(set.remove()).toBe(false);
    expect(set.count).toBe(1);
  });

  it("saves all, numbered from 1, and marks what was written as saved", async () => {
    const set = new ImageSet(open());
    draw(set.current.doc);
    set.add(open());
    draw(set.current.doc);
    set.add(open());
    draw(set.current.doc);
    expect(set.unsaved()).toEqual([1, 2, 3]);
    const numbers: number[] = [];
    // The second one fails to write
    const written = await set.saveAll(async (doc, number) => {
      numbers.push(number);
      return doc.flatten(0xffffffff).width > 0 && number !== 2;
    });
    expect(numbers).toEqual([1, 2, 3]);
    expect(written).toBe(2);
    expect(set.unsaved()).toEqual([2]);
  });

  it("tells subscribers when the set changes", () => {
    const set = new ImageSet(open());
    let calls = 0;
    const stop = set.subscribe(() => calls++);
    const page = set.add(open());
    set.setThumbnail(page, "data:a");
    set.setThumbnail(page, "data:a");
    set.show(0);
    stop();
    set.show(1);
    // add (show + add), thumbnail once, show
    expect(calls).toBe(4);
    expect(set.list[1].thumbnail).toBe("data:a");
  });
});

describe("what the window says", () => {
  it("names the shown image and the unsaved ones", () => {
    expect(positionLabel(1, 3)).toBe("Image 2 of 3");
    expect(closeQuestion([1], 1)).toBe("This image has changes that are not saved.");
    expect(closeQuestion([2], 3)).toBe("Image 2 of 3 has changes that are not saved. Closing the editor discards every image in it.");
    expect(closeQuestion([1, 3], 3)).toBe(
      "Images 1 and 3 of 3 have changes that are not saved. Closing the editor discards every image in it.",
    );
    expect(closeQuestion([1, 2, 4], 4)).toMatch(/^Images 1, 2 and 4 of 4 have/);
    expect(removeQuestion(0, 2)).toBe("Image 1 of 2 has changes that are not saved.");
  });
});
