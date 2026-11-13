import { describe, expect, it } from "vitest";
import { initialState, reduce, toolSize, type EditorSettings } from "./state";

const settings: EditorSettings = {
  color: [255, 0, 0],
  palette: [null, [255, 255, 255], [0, 0, 0]],
  sizes: { pencil: 3, text: 8, marker: 5 },
  thickness: 3,
  arrow_head: "default",
  reverse_arrow: false,
  shape_kind: "square",
  shape_style: "filled",
  rectangle_style: "filled",
  highlight_outline: 2,
  redact: "frosted",
  undo_limit: 100,
  shortcuts: {},
};

describe("editor state", () => {
  it("starts from the overlay's settings", () => {
    const state = initialState(settings);
    expect(state.color).toEqual({ r: 255, g: 0, b: 0 });
    expect(state.palette).toEqual([null, { r: 255, g: 255, b: 255 }, { r: 0, g: 0, b: 0 }]);
    expect(state.tool).toBeNull();
    expect(state.options.shapeStyle).toBe("filled");
  });

  it("picks a tool and puts it down when picked again", () => {
    let state = reduce(initialState(settings), { type: "select_tool", tool: "pencil" });
    expect(state.tool).toBe("pencil");
    state = reduce(state, { type: "select_tool", tool: "pencil" });
    expect(state.tool).toBeNull();
  });

  it("keeps a size per tool, within 1 to 50", () => {
    let state = reduce(initialState(settings), { type: "select_tool", tool: "text" });
    expect(toolSize(state)).toBe(8);
    state = reduce(state, { type: "set_size", size: 80 });
    expect(toolSize(state)).toBe(50);
    state = reduce(state, { type: "step_size", by: -60 });
    expect(toolSize(state)).toBe(1);
    state = reduce(state, { type: "select_tool", tool: "pencil" });
    expect(toolSize(state)).toBe(3);
    expect(state.sizes.text).toBe(1);
    // With no tool, the shared thickness
    state = reduce(state, { type: "select_tool", tool: null });
    state = reduce(state, { type: "set_size", size: 7 });
    expect(state.thickness).toBe(7);
  });

  it("applies a picked colour at once, with no confirm step", () => {
    let state = reduce(initialState(settings), { type: "open_picker" });
    expect(state.picker.open).toBe(true);
    // Every pointer move is a pick that applies straight away
    for (const g of [10, 20, 30]) {
      state = reduce(state, { type: "set_color", color: { r: 0, g, b: 0 } });
      expect(state.color).toEqual({ r: 0, g, b: 0 });
      expect(state.picker.open).toBe(true);
    }
  });

  it("keeps the last colour when the picker closes (Esc or a click away)", () => {
    let state = reduce(initialState(settings), { type: "open_picker" });
    state = reduce(state, { type: "set_color", color: { r: 1, g: 2, b: 3 } });
    state = reduce(state, { type: "eyedropper", on: true });
    state = reduce(state, { type: "close_picker" });
    expect(state.picker.open).toBe(false);
    expect(state.picker.eyedropper).toBe(false);
    expect(state.color).toEqual({ r: 1, g: 2, b: 3 });
  });

  it("remembers the hue while the colour is grey", () => {
    let state = reduce(initialState(settings), { type: "set_color", color: { r: 0, g: 0, b: 255 }, hue: 240 });
    state = reduce(state, { type: "set_color", color: { r: 90, g: 90, b: 90 } });
    expect(state.picker.hue).toBe(240);
  });

  it("puts the tool down while cropping", () => {
    let state = reduce(initialState(settings), { type: "select_tool", tool: "arrow" });
    state = reduce(state, { type: "crop", on: true });
    expect(state.cropping).toBe(true);
    expect(state.tool).toBeNull();
    state = reduce(state, { type: "select_tool", tool: "line" });
    expect(state.cropping).toBe(false);
  });
});
