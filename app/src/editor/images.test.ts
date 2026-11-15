import { beforeEach, describe, expect, it } from "vitest";
import { EditorController } from "./controller";
import type { Engine, Shape } from "./engine";
import {
  cleanName,
  closeQuestion,
  fileName,
  FORBIDDEN_HINT,
  hasForbidden,
  ImageSet,
  parseName,
  positionLabel,
  removeQuestion,
  withoutForbidden,
} from "./images";
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
    const set = new ImageSet(open(), "shot");
    const second = set.add(open(120), "shot");
    const third = set.add(open(140), "shot");
    expect(set.count).toBe(3);
    expect(set.position).toBe(2);
    expect(set.current).toBe(third);
    expect(set.list.map((page) => page.doc.session.width)).toEqual([100, 120, 140]);
    expect(new Set(set.list.map((page) => page.id)).size).toBe(3);
    expect(second.doc).not.toBe(third.doc);
  });

  it("keeps each image's own undo history and unsaved state", () => {
    const set = new ImageSet(open(), "shot");
    set.add(open(), "shot");
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
    const set = new ImageSet(open(), "shot");
    set.add(open(), "shot");
    const editor = set.current.doc;
    editor.startText({ x: 10, y: 10 }, RED, 10, {}, "left");
    editor.typeText("Hi");
    set.show(0);
    expect(editor.text).toBeNull();
    expect(editor.session.state().objects).toBe(1);
  });

  it("removes an image, shows its neighbour and never removes the last one", () => {
    const set = new ImageSet(open(100), "shot");
    set.add(open(120), "shot");
    set.add(open(140), "shot");
    set.show(1);
    expect(set.remove()).toBe(true);
    expect(set.list.map((page) => page.doc.session.width)).toEqual([100, 140]);
    // The one after it takes its place
    expect(set.current.doc.session.width).toBe(140);
    // The last of the list: the new last one shows
    expect(set.remove()).toBe(true);
    expect(set.current.doc.session.width).toBe(100);
    // Removing one before the shown image keeps the same image shown
    set.add(open(160), "shot");
    set.remove(0);
    expect(set.current.doc.session.width).toBe(160);
    expect(set.remove()).toBe(false);
    expect(set.count).toBe(1);
  });

  it("saves all, numbered from 1, and marks what was written as saved", async () => {
    const set = new ImageSet(open(), "shot");
    draw(set.current.doc);
    set.add(open(), "shot");
    draw(set.current.doc);
    set.add(open(), "shot");
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
    const set = new ImageSet(open(), "shot");
    let calls = 0;
    const stop = set.subscribe(() => calls++);
    const page = set.add(open(), "shot");
    set.setThumbnail(page, "data:a");
    set.setThumbnail(page, "data:a");
    set.show(0);
    stop();
    set.show(1);
    // add (show + add), thumbnail once, show
    expect(calls).toBe(4);
    expect(set.list[1].thumbnail).toBe("data:a");
  });

  /** Three images named a, b and c, in that order. */
  const abc = () => {
    const set = new ImageSet(open(100), "a");
    set.add(open(120), "b");
    set.add(open(140), "c");
    return set;
  };
  const names = (set: ImageSet<EditorController>) => set.list.map((page) => page.name);

  it("moves an image to the front or the end; the shown image stays shown", () => {
    const set = abc();
    set.show(1);
    const shown = set.current;
    // Another image moves: the shown one keeps showing, at its new place
    expect(set.moveToFront(2)).toBe(true);
    expect(names(set)).toEqual(["c", "a", "b"]);
    expect(set.current).toBe(shown);
    expect(set.position).toBe(2);
    // The shown image itself moves, and stays shown
    expect(set.moveToFront(2)).toBe(true);
    expect(names(set)).toEqual(["b", "c", "a"]);
    expect(set.current).toBe(shown);
    expect(set.position).toBe(0);
    expect(set.moveToEnd(0)).toBe(true);
    expect(names(set)).toEqual(["c", "a", "b"]);
    expect(set.position).toBe(2);
    // Already there, or no such image: nothing changes
    expect(set.moveToFront(0)).toBe(false);
    expect(set.moveToEnd(2)).toBe(false);
    expect(set.moveToEnd(5)).toBe(false);
    expect(names(set)).toEqual(["c", "a", "b"]);
  });

  it("keeps each image's own history and unsaved state through a move", () => {
    const set = abc();
    draw(set.current.doc);
    const drawn = set.current;
    set.moveToFront(2);
    expect(set.list[0]).toBe(drawn);
    expect(set.list[0].doc.canUndo).toBe(true);
    expect(set.unsaved()).toEqual([1]);
    set.show(1);
    expect(set.current.doc.canUndo).toBe(false);
  });

  it("saves all in the strip's order, each image under its own name", async () => {
    const set = abc();
    set.moveToFront(2);
    set.rename(1, "first shot");
    const saved: [number, string, number][] = [];
    await set.saveAll(async (doc, number, name) => {
      saved.push([number, name, doc.session.width]);
      return true;
    });
    expect(saved).toEqual([
      [1, "c", 140],
      [2, "first shot", 100],
      [3, "b", 120],
    ]);
  });

  it("gives new images the format they are added with, PNG by default", () => {
    const set = new ImageSet(open(), "a", "webp");
    set.add(open(), "b", "pdf");
    set.add(open(), "c");
    expect(set.list.map((page) => fileName(page))).toEqual(["a.webp", "b.pdf", "c.png"]);
  });

  it("converts an image: only its format changes, and that is a change to save", () => {
    const set = abc();
    const pixels = set.list[0].doc.session.width;
    let calls = 0;
    set.subscribe(() => calls++);
    expect(set.setFormat(0, "pdf")).toBe(true);
    expect(set.list[0].format).toBe("pdf");
    expect(set.list[0].name).toBe("a");
    expect(set.list[0].doc.session.width).toBe(pixels);
    expect(set.list[0].doc.canUndo).toBe(false);
    expect(set.unsaved()).toEqual([1]);
    expect(calls).toBe(1);
    // The format it has already, or no such image: nothing happens
    expect(set.setFormat(0, "pdf")).toBe(false);
    expect(set.setFormat(7, "png")).toBe(false);
    expect(calls).toBe(1);
  });

  it("saves all, each image in its own format", async () => {
    const set = abc();
    set.setFormat(0, "jpg");
    set.setFormat(2, "pdf");
    const saved: string[] = [];
    await set.saveAll(async (_doc, number, name, format) => {
      saved.push(`${number} - ${name}.${format}`);
      return true;
    });
    expect(saved).toEqual(["1 - a.jpg", "2 - b.png", "3 - c.pdf"]);
    expect(set.unsaved()).toEqual([]);
  });

  it("renames with a known extension to convert, and refuses another format's", () => {
    const set = abc();
    // The field shows the name with its extension
    expect(set.rename(0, "Login bug.png")).toBe(true);
    expect([set.list[0].name, set.list[0].format]).toEqual(["Login bug", "png"]);
    expect(set.unsaved()).toEqual([]);
    expect(set.rename(0, "Login bug.PDF")).toBe(true);
    expect([set.list[0].name, set.list[0].format]).toEqual(["Login bug", "pdf"]);
    expect(set.unsaved()).toEqual([1]);
    expect(set.rename(1, "report.jpeg")).toBe(true);
    expect(fileName(set.list[1])).toBe("report.jpg");
    // Only the extension: the name stays, the format changes
    expect(set.rename(2, ".webp")).toBe(true);
    expect(fileName(set.list[2])).toBe("c.webp");
    // Another image format: refused, nothing changes
    expect(set.rename(1, "report.tif")).toBe(false);
    expect(set.rename(1, "report.GIF")).toBe(false);
    expect(fileName(set.list[1])).toBe("report.jpg");
    // Other dots are part of the name; the format stays
    expect(set.rename(1, "v1.2")).toBe(true);
    expect(fileName(set.list[1])).toBe("v1.2.jpg");
    expect(set.rename(1, "v1.2.bmp")).toBe(true);
    expect(fileName(set.list[1])).toBe("v1.2.bmp");
  });

  it("renames an image by the rename rules", () => {
    const set = abc();
    let calls = 0;
    set.subscribe(() => calls++);
    expect(set.rename(0, "Login bug")).toBe(true);
    expect(set.list[0].name).toBe("Login bug");
    expect(calls).toBe(1);
    // Empty after trimming: the name stays, and nothing is told
    expect(set.rename(0, " .. ")).toBe(false);
    expect(set.list[0].name).toBe("Login bug");
    expect(set.rename(9, "x")).toBe(false);
    expect(calls).toBe(1);
  });
});

describe("image names", () => {
  it("refuses the characters Windows forbids in file names", () => {
    for (const character of ["\\", "/", ":", "*", "?", '"', "<", ">", "|", "\u0007"]) {
      expect(hasForbidden(`a${character}b`)).toBe(true);
    }
    expect(hasForbidden("Bug report v1.2 (été)")).toBe(false);
    expect(withoutForbidden('a/b:c*d?e"f<g>h|i\\j')).toBe("abcdefghij");
    expect(FORBIDDEN_HINT).toBe('A name cannot contain \\ / : * ? " < > |');
  });

  it("splits a typed name into the name and a format, or refuses another format", () => {
    expect(parseName("shot.png")).toEqual({ name: "shot", format: "png" });
    expect(parseName("shot.JPG")).toEqual({ name: "shot", format: "jpg" });
    expect(parseName("shot.Jpeg")).toEqual({ name: "shot", format: "jpg" });
    expect(parseName("shot.bmp")).toEqual({ name: "shot", format: "bmp" });
    expect(parseName("shot.webp")).toEqual({ name: "shot", format: "webp" });
    expect(parseName("shot.pdf. ")).toEqual({ name: "shot", format: "pdf" });
    expect(parseName("a.b.pdf")).toEqual({ name: "a.b", format: "pdf" });
    for (const other of ["tif", "TIFF", "gif", "heic", "ico", "svg"]) {
      expect(parseName(`shot.${other}`)).toEqual({ refused: other });
    }
    // Not a format: a dot in the name
    expect(parseName("v1.2")).toEqual({ name: "v1.2" });
    expect(parseName("notes.final")).toEqual({ name: "notes.final" });
    expect(parseName("plain")).toEqual({ name: "plain" });
  });

  it("trims trailing dots and spaces, and an empty name brings the previous one back", () => {
    expect(cleanName("notes. . ", "old")).toBe("notes");
    expect(cleanName("  leading stays", "old")).toBe("  leading stays");
    expect(cleanName("v1.2", "old")).toBe("v1.2");
    expect(cleanName("", "old")).toBe("old");
    expect(cleanName(" . ", "old")).toBe("old");
    expect(cleanName("a/b", "old")).toBe("ab");
    expect(cleanName("///", "old")).toBe("old");
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
