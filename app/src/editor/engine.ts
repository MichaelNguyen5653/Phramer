// The annotation engine (crates/phramer-annotate-wasm) in the page: the same
// Rust code the capture overlay draws with, so the editor's pixels are the
// overlay's. Requests go in as JSON through the module's staging buffer;
// answers and renderings come back through each session's output buffer.

export interface Rgba {
  r: number;
  g: number;
  b: number;
  a?: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export type ArrowHead = "default" | "curved";
export type ShapeStyle = "outline" | "filled" | "highlight";
export type RedactStyle = "frosted" | "insecure";
export type TextAlign = "left" | "center" | "right";

export interface TextFont {
  family?: string;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strikeout?: boolean;
}

/** An annotation's geometry, as the engine serialises `Shape`. */
export type Shape =
  | { tool: "pencil"; points: Point[] }
  | { tool: "line"; start: Point; end: Point }
  | { tool: "arrow"; start: Point; end: Point; head: ArrowHead; reversed: boolean }
  | { tool: "rectangle"; start: Point; end: Point; style: ShapeStyle; outline_width: number }
  | { tool: "ellipse"; start: Point; end: Point; style: ShapeStyle; outline_width: number }
  | { tool: "marker"; start: Point; end: Point }
  | { tool: "text"; top_left: Point; text: string; font: TextFont; align: TextAlign }
  | { tool: "pixelate"; start: Point; end: Point; style: RedactStyle }
  | { tool: "invert"; start: Point; end: Point }
  | { tool: "circle_counter"; center: Point; pointer: Point; count: number };

export interface Annotation {
  id: number;
  color: Rgba;
  size: number;
  shape: Shape;
}

/** One edit (`Command`). */
export type Edit =
  | { kind: "add"; color: Rgba; size: number; shape: Shape }
  | { kind: "move"; id: number; dx: number; dy: number }
  | { kind: "reshape"; id: number; shape: Shape }
  | { kind: "set_color"; id: number; color: Rgba }
  | { kind: "set_size"; id: number; size: number }
  | { kind: "delete"; id: number };

export interface EngineState {
  can_undo: boolean;
  can_redo: boolean;
  next_counter: number;
  objects: number;
}

/** A rendering: where it sits in image coordinates and its straight RGBA. */
export interface Rendered extends Rect {
  rgba: Uint8ClampedArray<ArrayBuffer>;
}

interface Exports {
  memory: WebAssembly.Memory;
  pa_stage(len: number): number;
  pa_session_new(width: number, height: number, bundledFont: number): number;
  pa_session_free(handle: number): void;
  pa_add_font(handle: number, bold: number, italic: number, familyLen: number, index: number): number;
  pa_call(handle: number): number;
  pa_render(handle: number, workspace: number): number;
  pa_out_ptr(handle: number): number;
  pa_out_len(handle: number): number;
}

const OK = 0;
const RENDER_HEADER = 16;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** One face of a font family, as the daemon reads it from the system
 * (`editor_font_faces`); `index` is its place in a collection (.ttc). */
export interface FontFace {
  family: string;
  bold: boolean;
  italic: boolean;
  index: number;
  data: Uint8Array;
}

/** The loaded module. One per page; sessions share it. */
export class Engine {
  /** Faces every session gets, the ones made later included. */
  private readonly fonts: FontFace[] = [];
  private readonly live = new Set<Session>();

  private constructor(private readonly exports: Exports) {}

  /** Adds `face` to every session, now and to come. A face that cannot be
   * read is left out and reported. */
  addFont(face: FontFace): void {
    this.fonts.push(face);
    for (const session of this.live) Engine.register(session, face);
  }

  private static register(session: Session, face: FontFace): void {
    try {
      session.addFont(face.family, face.data, face.bold, face.italic, face.index);
    } catch (error) {
      console.warn(`font ${face.family}: ${String(error)}`);
    }
  }

  hasFont(family: string): boolean {
    return this.fonts.some((face) => face.family.toLowerCase() === family.toLowerCase());
  }

  forget(session: Session): void {
    this.live.delete(session);
  }

  /** Instantiates the module from its bytes. */
  static async fromBytes(bytes: BufferSource): Promise<Engine> {
    const { instance } = await WebAssembly.instantiate(bytes, {});
    return new Engine(instance.exports as unknown as Exports);
  }

  /** Fetches and instantiates the module. */
  static async load(url: string): Promise<Engine> {
    const response = await fetch(url);
    if (typeof WebAssembly.instantiateStreaming === "function") {
      try {
        const { instance } = await WebAssembly.instantiateStreaming(response.clone(), {});
        return new Engine(instance.exports as unknown as Exports);
      } catch {
        // A server that sends the wrong MIME type: fall back to bytes
      }
    }
    return Engine.fromBytes(await response.arrayBuffer());
  }

  /** Copies `bytes` into the module's staging buffer. */
  stage(bytes: Uint8Array): void {
    const ptr = this.exports.pa_stage(bytes.length) >>> 0;
    new Uint8Array(this.exports.memory.buffer, ptr, bytes.length).set(bytes);
  }

  out(handle: number): Uint8Array {
    const ptr = this.exports.pa_out_ptr(handle) >>> 0;
    const len = this.exports.pa_out_len(handle) >>> 0;
    return new Uint8Array(this.exports.memory.buffer, ptr, len);
  }

  /** A session over straight RGBA pixels; the overlay's font unless told otherwise. */
  session(width: number, height: number, rgba: Uint8Array, bundledFont = true): Session {
    this.stage(rgba);
    const handle = this.exports.pa_session_new(width, height, bundledFont ? 1 : 0);
    if (handle === 0) throw new Error(`no session for a ${width}x${height} image`);
    const session = new Session(this, this.exports, handle, width, height);
    this.live.add(session);
    for (const face of this.fonts) Engine.register(session, face);
    return session;
  }
}

/** One image being edited, with its undo history, inside the module. */
export class Session {
  constructor(
    private readonly engine: Engine,
    private readonly exports: Exports,
    readonly handle: number,
    readonly width: number,
    readonly height: number,
  ) {}

  addFont(family: string, data: Uint8Array, bold = false, italic = false, index = 0): void {
    const name = encoder.encode(family);
    const staged = new Uint8Array(name.length + data.length);
    staged.set(name);
    staged.set(data, name.length);
    this.engine.stage(staged);
    if (this.exports.pa_add_font(this.handle, bold ? 1 : 0, italic ? 1 : 0, name.length, index) !== OK) {
      throw new Error(decoder.decode(this.engine.out(this.handle)));
    }
  }

  /** Answers one request (see `Request` in the crate). */
  call<T>(request: object): T {
    this.engine.stage(encoder.encode(JSON.stringify(request)));
    const status = this.exports.pa_call(this.handle);
    const answer = JSON.parse(decoder.decode(this.engine.out(this.handle)));
    if (status !== OK) throw new Error(String(answer));
    return answer as T;
  }

  apply(edit: Edit): { id: number | null; recorded: boolean; state: EngineState } {
    return this.call({ op: "apply", edit });
  }

  undo(): { done: boolean; state: EngineState } {
    return this.call({ op: "undo" });
  }

  redo(): { done: boolean; state: EngineState } {
    return this.call({ op: "redo" });
  }

  state(): EngineState {
    return this.call({ op: "state" });
  }

  setNextCounter(next: number): EngineState {
    return this.call({ op: "set_next_counter", next });
  }

  setUndoLimit(limit: number): EngineState {
    return this.call({ op: "set_undo_limit", limit });
  }

  hitTest(point: Point): number | null {
    return this.call({ op: "hit_test", x: point.x, y: point.y });
  }

  object(id: number): Annotation | null {
    return this.call({ op: "object", id });
  }

  bounds(id: number, preview: Edit[] = []): Rect | null {
    return this.call({ op: "bounds", id, preview });
  }

  annotationBounds(annotation: Omit<Annotation, "id">): Rect | null {
    return this.call({ op: "annotation_bounds", annotation });
  }

  textTopLeft(at: Point, size: number, font: TextFont = {}): Point {
    return this.call({ op: "text_top_left", x: at.x, y: at.y, size, font });
  }

  /** The overlay's Shift rule for a drag with `tool` (an engine tool kind). */
  constrain(start: Point, cursor: Point, tool: Shape["tool"]): Point {
    return this.call({ op: "constrain", start, cursor, tool });
  }

  renderedBounds(preview: Edit[] = []): Rect {
    return this.call({ op: "rendered_bounds", preview });
  }

  objects(): Annotation[] {
    return this.call<{ objects?: Annotation[] }>({ op: "document" }).objects ?? [];
  }

  /** The image with every annotation and `preview` on top. `workspace` is
   * the colour past the image, 0xRRGGBBAA. */
  render(preview: Edit[] = [], workspace = 0xffffffff): Rendered {
    this.engine.stage(preview.length ? encoder.encode(JSON.stringify(preview)) : new Uint8Array(0));
    const status = this.exports.pa_render(this.handle, workspace >>> 0);
    const out = this.engine.out(this.handle);
    if (status !== OK) throw new Error(decoder.decode(out));
    const view = new DataView(out.buffer, out.byteOffset, RENDER_HEADER);
    const x = view.getInt32(0, true);
    const y = view.getInt32(4, true);
    const width = view.getUint32(8, true);
    const height = view.getUint32(12, true);
    // A copy: the module's memory moves on the next call
    const rgba = new Uint8ClampedArray(out.subarray(RENDER_HEADER, RENDER_HEADER + width * height * 4));
    return { x, y, width, height, rgba };
  }

  free(): void {
    this.engine.forget(this);
    this.exports.pa_session_free(this.handle);
  }
}

/** 0xRRGGBBAA of a colour. */
export function packRgba(color: Rgba): number {
  return (((color.r & 255) << 24) | ((color.g & 255) << 16) | ((color.b & 255) << 8) | ((color.a ?? 255) & 255)) >>> 0;
}
