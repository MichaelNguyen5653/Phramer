// The images of one editor window: 15.0.0's editor session. Each image has
// its own document (undo history, unsaved state); one is shown at a time and
// the filmstrip shows them all. The UI reads it and calls in.

import type { View } from "./zoom";

/** What the set needs of an image's document (`EditorController`). */
export interface Doc {
  dirty: boolean;
  /** Places text still being typed. */
  commitText(): void;
  markSaved(): void;
  /** Frees what the document holds; it is not used again. */
  dispose(): void;
}

export interface Page<D extends Doc> {
  readonly id: number;
  readonly doc: D;
  /** The view it was left at, and the image (session) that view is of. */
  view?: { view: View; of: unknown };
  /** A small picture of it for the filmstrip (a data URL). */
  thumbnail?: string;
}

export class ImageSet<D extends Doc> {
  private pages: Page<D>[] = [];
  private shown = 0;
  private nextId = 0;
  private listeners = new Set<() => void>();
  private version = 0;

  constructor(first: D) {
    this.pages.push({ id: this.nextId++, doc: first });
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

  // --- Reading ------------------------------------------------------------------

  get list(): readonly Page<D>[] {
    return this.pages;
  }

  get count(): number {
    return this.pages.length;
  }

  /** The index of the image shown. */
  get position(): number {
    return this.shown;
  }

  get current(): Page<D> {
    return this.pages[this.shown];
  }

  /** The images with changes that are not saved, numbered from 1. */
  unsaved(): number[] {
    return this.pages.flatMap((page, index) => (page.doc.dirty ? [index + 1] : []));
  }

  // --- Changing -----------------------------------------------------------------

  /** A new image, at the end, shown (15.0.0: a capture joins the session). */
  add(doc: D): Page<D> {
    const page = { id: this.nextId++, doc };
    this.pages.push(page);
    this.show(this.pages.length - 1);
    this.changed();
    return page;
  }

  /** Shows the image at `index`. Leaving an image places its typed text. */
  show(index: number): boolean {
    if (index < 0 || index >= this.pages.length) return false;
    if (index !== this.shown) this.current.doc.commitText();
    this.shown = index;
    this.changed();
    return true;
  }

  /** Removes the image at `index`; the one after it (or the new last one)
   * is shown. The last image is never removed: the window closes instead. */
  remove(index = this.shown): boolean {
    if (this.pages.length <= 1 || index < 0 || index >= this.pages.length) return false;
    const [removed] = this.pages.splice(index, 1);
    removed.doc.dispose();
    if (index < this.shown || this.shown >= this.pages.length) this.shown--;
    this.shown = Math.max(0, Math.min(this.shown, this.pages.length - 1));
    this.changed();
    return true;
  }

  setThumbnail(page: Page<D>, thumbnail: string): void {
    if (page.thumbnail === thumbnail) return;
    page.thumbnail = thumbnail;
    this.changed();
  }

  /** Save All: every image in order, numbered from 1. The ones `save`
   * wrote are saved; returns how many that was. */
  async saveAll(save: (doc: D, number: number) => Promise<boolean>): Promise<number> {
    let written = 0;
    for (const [index, page] of [...this.pages].entries()) {
      page.doc.commitText();
      if (await save(page.doc, index + 1)) {
        page.doc.markSaved();
        written++;
      }
    }
    this.changed();
    return written;
  }
}

/** `1`, `1 and 3`, `1, 2 and 4`. */
function numbers(list: number[]): string {
  if (list.length <= 1) return list.join("");
  return `${list.slice(0, -1).join(", ")} and ${list[list.length - 1]}`;
}

/** Where the shown image is, for the status bar. */
export function positionLabel(position: number, count: number): string {
  return `Image ${position + 1} of ${count}`;
}

/** What closing asks when images are not saved: which ones. */
export function closeQuestion(unsaved: number[], count: number): string {
  if (count <= 1) return "This image has changes that are not saved.";
  const which = unsaved.length === 1 ? `Image ${unsaved[0]} of ${count} has` : `Images ${numbers(unsaved)} of ${count} have`;
  return `${which} changes that are not saved. Closing the editor discards every image in it.`;
}

/** What Remove Image asks when the image is not saved. */
export function removeQuestion(position: number, count: number): string {
  return `Image ${position + 1} of ${count} has changes that are not saved.`;
}
