// The document side of the editor: the engine session, what is being
// drawn, moved or typed, and one history for undo that covers edits, colour
// picks and crops. The UI reads it and calls in; it never touches pixels.

import type { Rgb } from "./color";
import { sameRgb } from "./color";
import type { Annotation, Edit, Engine, Point, Rect, Rendered, Rgba, Session, Shape, TextAlign, TextFont } from "./engine";

/** One step of undo. `doc` steps are the engine's; a colour pick is
 * remembered so undo gives the old colour back; a crop swaps sessions. */
interface Entry {
  doc: boolean;
  color?: { before: Rgb; after: Rgb };
  crop?: { before: Session; after: Session };
}

export interface TextEdit {
  topLeft: Point;
  text: string;
  /** In characters. */
  caret: number;
  color: Rgba;
  size: number;
  font: TextFont;
  align: TextAlign;
}

export interface UndoResult {
  /** The draw colour to go back (or forward) to, when the step was a pick. */
  color?: Rgb;
}

export class EditorController {
  session: Session;
  /** The object selected for moving, recolouring, resizing, deleting. */
  selected: number | null = null;
  /** The object being drawn. */
  draft: { color: Rgba; size: number; shape: Shape } | null = null;
  /** The selected object's offset while dragged. */
  moving: { dx: number; dy: number } | null = null;
  /** A colour or size shown on the selected object before it is kept. */
  restyle: { color?: Rgba; size?: number } | null = null;
  text: TextEdit | null = null;
  /** Changes since the image was last written to a file. */
  dirty = false;

  private undoStack: Entry[] = [];
  private redoStack: Entry[] = [];
  private listeners = new Set<() => void>();
  private version = 0;

  constructor(
    private readonly engine: Engine,
    session: Session,
    private limit: number,
  ) {
    this.session = session;
    this.session.setUndoLimit(limit);
  }

  static open(engine: Engine, width: number, height: number, rgba: Uint8Array, undoLimit: number): EditorController {
    return new EditorController(engine, engine.session(width, height, rgba), undoLimit);
  }

  // --- Change notification (useSyncExternalStore) ----------------------------

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getVersion = (): number => this.version;

  changed(): void {
    this.version++;
    for (const listener of this.listeners) listener();
  }

  // --- History ----------------------------------------------------------------

  get canUndo(): boolean {
    return this.undoStack.length > 0;
  }

  get canRedo(): boolean {
    return this.redoStack.length > 0;
  }

  private record(entry: Entry): void {
    this.undoStack.push(entry);
    for (const dropped of this.redoStack) {
      if (dropped.crop) dropped.crop.after.free();
    }
    this.redoStack = [];
    if (this.limit > 0) {
      while (this.undoStack.length > this.limit) this.undoStack.shift();
    }
    this.dirty = true;
  }

  /** Applies an edit as one undo step; returns the new object's id. */
  apply(edit: Edit, color?: { before: Rgb; after: Rgb }): number | null {
    const result = this.session.apply(edit);
    // An edit that changed nothing is not recorded by the engine either
    if (result.recorded) {
      this.record({ doc: true, color });
    }
    this.forgetMissing();
    this.changed();
    return result.id;
  }

  undo(): UndoResult {
    this.commitText();
    const entry = this.undoStack.pop();
    if (!entry) return {};
    if (entry.doc) this.session.undo();
    if (entry.crop) this.session = entry.crop.before;
    this.redoStack.push(entry);
    this.dirty = true;
    this.forgetMissing();
    this.changed();
    return { color: entry.color?.before };
  }

  redo(): UndoResult {
    const entry = this.redoStack.pop();
    if (!entry) return {};
    if (entry.crop) this.session = entry.crop.after;
    if (entry.doc) this.session.redo();
    this.undoStack.push(entry);
    this.dirty = true;
    this.forgetMissing();
    this.changed();
    return { color: entry.color?.after };
  }

  private forgetMissing(): void {
    if (this.selected !== null && this.session.object(this.selected) === null) this.selected = null;
  }

  // --- Drawing ------------------------------------------------------------------

  startDraft(color: Rgba, size: number, shape: Shape): void {
    this.selected = null;
    this.draft = { color, size, shape };
    this.changed();
  }

  updateDraft(shape: Shape): void {
    if (!this.draft) return;
    this.draft = { ...this.draft, shape };
    this.changed();
  }

  /** The drag ended: the drawing becomes an object, unless it draws nothing. */
  finishDraft(): number | null {
    const draft = this.draft;
    this.draft = null;
    if (!draft) return null;
    try {
      return this.apply({ kind: "add", ...draft });
    } catch {
      // A click that drew nothing is no object
      this.changed();
      return null;
    }
  }

  // --- Selection ------------------------------------------------------------------

  select(id: number | null): void {
    this.selected = id;
    this.moving = null;
    this.restyle = null;
    this.changed();
  }

  hitTest(at: Point): number | null {
    return this.session.hitTest(at);
  }

  moveBy(dx: number, dy: number): void {
    this.moving = { dx, dy };
    this.changed();
  }

  finishMove(): void {
    const moving = this.moving;
    this.moving = null;
    if (this.selected !== null && moving && (moving.dx !== 0 || moving.dy !== 0)) {
      this.apply({ kind: "move", id: this.selected, ...moving });
    } else {
      this.changed();
    }
  }

  deleteSelected(): void {
    if (this.selected === null) return;
    const id = this.selected;
    this.selected = null;
    this.apply({ kind: "delete", id });
  }

  selectedObject(): Annotation | null {
    return this.selected === null ? null : this.session.object(this.selected);
  }

  /** The selected object's outline, where it is drawn now. */
  selectedBounds(): Rect | null {
    if (this.selected === null) return null;
    const preview: Edit[] = this.moving ? [{ kind: "move", id: this.selected, ...this.moving }] : [];
    if (this.restyle?.size !== undefined) preview.push({ kind: "set_size", id: this.selected, size: this.restyle.size });
    return this.session.bounds(this.selected, preview);
  }

  // --- Colour and size ------------------------------------------------------------

  /** Shows `color` on the selected object while it is being picked. */
  previewColor(color: Rgb): void {
    if (this.selected === null) return;
    this.restyle = { ...this.restyle, color: { ...color, a: 255 } };
    this.changed();
  }

  /** A pick is finished: one undo step, recolouring the selected object if
   * there is one. Nothing is recorded when the colour did not change. */
  commitColor(before: Rgb, after: Rgb): void {
    const restyle = this.restyle;
    this.restyle = restyle?.size !== undefined ? { size: restyle.size } : null;
    if (this.text) {
      this.text = { ...this.text, color: { ...after, a: 255 } };
    }
    const object = this.selectedObject();
    if (object) {
      const current = object.color;
      if (current.r !== after.r || current.g !== after.g || current.b !== after.b) {
        this.apply({ kind: "set_color", id: object.id, color: { ...after, a: 255 } }, { before, after });
        return;
      }
    }
    if (!sameRgb(before, after)) {
      this.record({ doc: false, color: { before, after } });
    }
    this.changed();
  }

  previewSize(size: number): void {
    if (this.selected === null) return;
    this.restyle = { ...this.restyle, size };
    this.changed();
  }

  commitSize(size: number): void {
    this.restyle = this.restyle?.color ? { color: this.restyle.color } : null;
    if (this.text) this.text = { ...this.text, size };
    const object = this.selectedObject();
    if (object && object.size !== size) {
      this.apply({ kind: "set_size", id: object.id, size });
    } else {
      this.changed();
    }
  }

  // --- Text -------------------------------------------------------------------------

  startText(at: Point, color: Rgba, size: number, font: TextFont, align: TextAlign): void {
    this.commitText();
    this.selected = null;
    const topLeft = this.session.textTopLeft(at, size, font);
    this.text = { topLeft, text: "", caret: 0, color, size, font, align };
    this.changed();
  }

  typeText(typed: string): void {
    const edit = this.text;
    if (!edit) return;
    const chars = [...edit.text];
    const clean = [...typed.replace(/\r\n?/g, "\n")].filter((c) => c === "\n" || c >= " ");
    chars.splice(edit.caret, 0, ...clean);
    this.text = { ...edit, text: chars.join(""), caret: edit.caret + clean.length };
    this.changed();
  }

  /** Keys while typing (`text_key`); returns false for keys it leaves alone. */
  textKey(key: "backspace" | "delete" | "left" | "right" | "home" | "end"): boolean {
    const edit = this.text;
    if (!edit) return false;
    const chars = [...edit.text];
    let caret = edit.caret;
    switch (key) {
      case "backspace":
        if (caret === 0) return true;
        chars.splice(caret - 1, 1);
        caret--;
        break;
      case "delete":
        chars.splice(caret, 1);
        break;
      case "left":
        caret = Math.max(0, caret - 1);
        break;
      case "right":
        caret = Math.min(chars.length, caret + 1);
        break;
      case "home": {
        const before = chars.slice(0, caret).join("");
        caret = before.lastIndexOf("\n") + 1;
        break;
      }
      case "end": {
        const after = chars.slice(caret).join("");
        const newline = after.indexOf("\n");
        caret += newline < 0 ? [...after].length : [...after.slice(0, newline)].length;
        break;
      }
    }
    this.text = { ...edit, text: chars.join(""), caret };
    this.changed();
    return true;
  }

  textShape(edit: TextEdit, text = edit.text): Shape {
    return { tool: "text", top_left: edit.topLeft, text, font: edit.font, align: edit.align };
  }

  /** Places the typed text; empty text places nothing. */
  commitText(): void {
    const edit = this.text;
    if (!edit) return;
    this.text = null;
    if (edit.text.length > 0) {
      this.apply({ kind: "add", color: edit.color, size: edit.size, shape: this.textShape(edit) });
    } else {
      this.changed();
    }
  }

  /** The caret's top-left in image pixels and its height (the overlay's
   * `caret`). */
  caret(): { at: Point; height: number } | null {
    const edit = this.text;
    if (!edit) return null;
    const before = [...edit.text].slice(0, edit.caret).join("");
    const lineIndex = before.split("\n").length - 1;
    const line = before.slice(before.lastIndexOf("\n") + 1);
    const area = (text: string) =>
      this.session.annotationBounds({ color: edit.color, size: edit.size, shape: this.textShape(edit, text) });
    const single = area("X");
    if (!single) return null;
    const lineHeight = Math.max(0, single.height - 10);
    const width = line ? Math.max(0, (area(line)?.width ?? 10) - 10) : 0;
    let advance = 0;
    if (lineIndex > 0) {
      const lines = "X\n".repeat(lineIndex) + "X";
      advance = Math.floor(Math.max(0, (area(lines)?.height ?? single.height) - single.height) / lineIndex);
    }
    return {
      at: { x: edit.topLeft.x + 5 + width, y: edit.topLeft.y + 5 + advance * lineIndex },
      height: lineHeight,
    };
  }

  // --- Rendering ----------------------------------------------------------------------

  /** What is drawn over the document: drag, recolour, drawing, typing. */
  previewEdits(): Edit[] {
    const edits: Edit[] = [];
    if (this.selected !== null) {
      if (this.moving) edits.push({ kind: "move", id: this.selected, ...this.moving });
      if (this.restyle?.color) edits.push({ kind: "set_color", id: this.selected, color: this.restyle.color });
      if (this.restyle?.size !== undefined) edits.push({ kind: "set_size", id: this.selected, size: this.restyle.size });
    }
    if (this.draft) edits.push({ kind: "add", ...this.draft });
    if (this.text && this.text.text) {
      edits.push({ kind: "add", color: this.text.color, size: this.text.size, shape: this.textShape(this.text) });
    }
    return edits;
  }

  render(workspace: number, withPreview = true): Rendered {
    return this.session.render(withPreview ? this.previewEdits() : [], workspace);
  }

  /** The image as it will be saved: text being typed is placed first. */
  flatten(workspace: number): Rendered {
    this.commitText();
    return this.render(workspace, false);
  }

  /** Crops to `rect` (image pixels, within the rendering): the annotations
   * become pixels of the new image. Undo gives the old image back. */
  crop(rect: Rect, workspace: number): boolean {
    const image = this.flatten(workspace);
    const left = Math.max(rect.x, image.x);
    const top = Math.max(rect.y, image.y);
    const right = Math.min(rect.x + rect.width, image.x + image.width);
    const bottom = Math.min(rect.y + rect.height, image.y + image.height);
    const width = Math.round(right - left);
    const height = Math.round(bottom - top);
    if (width < 1 || height < 1) return false;
    const rgba = new Uint8Array(width * height * 4);
    for (let row = 0; row < height; row++) {
      const from = ((top - image.y + row) * image.width + (left - image.x)) * 4;
      rgba.set(image.rgba.subarray(from, from + width * 4), row * width * 4);
    }
    const before = this.session;
    const after = this.engine.session(width, height, rgba);
    after.setUndoLimit(this.limit);
    this.session = after;
    this.selected = null;
    this.record({ doc: false, crop: { before, after } });
    this.changed();
    return true;
  }

  /** The colour of the rendered pixel at `at` (image pixels), for the
   * eyedropper. */
  static sample(rendered: Rendered, at: Point): Rgb | null {
    const x = Math.floor(at.x) - rendered.x;
    const y = Math.floor(at.y) - rendered.y;
    if (x < 0 || y < 0 || x >= rendered.width || y >= rendered.height) return null;
    const i = (y * rendered.width + x) * 4;
    return { r: rendered.rgba[i], g: rendered.rgba[i + 1], b: rendered.rgba[i + 2] };
  }

  markSaved(): void {
    this.dirty = false;
    this.changed();
  }
}
