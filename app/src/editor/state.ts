// The editor's UI state: the tool, its options, the colour and the view.
// Pure: every change is an action through `reduce`. What is drawn lives in
// the engine (controller.ts).

import type { Rgb } from "./color";
import { rgbToHsv } from "./color";
import type { ArrowHead, RedactStyle, ShapeStyle } from "./engine";
import { MAX_TOOL_SIZE, type ShapeKind, type ToolId, type ToolOptions } from "./tools";
import type { View } from "./zoom";

/** The tool settings the daemon reads from the settings file
 * (`editor_settings`). */
export interface EditorSettings {
  color: [number, number, number];
  palette: ([number, number, number] | null)[];
  sizes: Partial<Record<ToolId, number>>;
  thickness: number;
  arrow_head: ArrowHead;
  reverse_arrow: boolean;
  shape_kind: ShapeKind;
  shape_style: ShapeStyle;
  rectangle_style: ShapeStyle;
  highlight_outline: number;
  redact: RedactStyle;
  undo_limit: number;
  shortcuts: Record<string, string>;
}

export interface Picker {
  open: boolean;
  /** Where it opened, in viewport CSS pixels; `null` under the colour button. */
  anchor: { x: number; y: number } | null;
  /** The hue the square shows, kept while the colour is grey. */
  hue: number;
  /** Picking from the image: the colour follows the pointer. */
  eyedropper: boolean;
}

export interface UiState {
  tool: ToolId | null;
  color: Rgb;
  palette: (Rgb | null)[];
  sizes: Partial<Record<ToolId, number>>;
  /** The size with no tool selected (`drawThickness`). */
  thickness: number;
  options: ToolOptions;
  view: View;
  picker: Picker;
  cropping: boolean;
}

export type Action =
  | { type: "select_tool"; tool: ToolId | null }
  | { type: "set_size"; size: number }
  | { type: "step_size"; by: number }
  | { type: "set_options"; options: Partial<ToolOptions> }
  | { type: "set_color"; color: Rgb; hue?: number }
  | { type: "open_picker"; anchor?: { x: number; y: number } | null }
  | { type: "close_picker" }
  | { type: "eyedropper"; on: boolean }
  | { type: "set_view"; view: View }
  | { type: "crop"; on: boolean };

const rgb = ([r, g, b]: [number, number, number]): Rgb => ({ r, g, b });

export function initialState(settings: EditorSettings): UiState {
  const color = rgb(settings.color);
  return {
    tool: null,
    color,
    palette: settings.palette.map((entry) => (entry ? rgb(entry) : null)),
    sizes: { ...settings.sizes },
    thickness: settings.thickness,
    options: {
      arrowHead: settings.arrow_head,
      reverseArrow: settings.reverse_arrow,
      shapeKind: settings.shape_kind,
      shapeStyle: settings.shape_style,
      rectangleStyle: settings.rectangle_style,
      highlightOutline: settings.highlight_outline,
      redact: settings.redact,
      textAlign: "left",
      underline: false,
      strikeout: false,
    },
    view: { scale: 1, x: 0, y: 0 },
    picker: { open: false, anchor: null, hue: rgbToHsv(color).h, eyedropper: false },
    cropping: false,
  };
}

export function clampSize(size: number): number {
  return Math.min(MAX_TOOL_SIZE, Math.max(1, Math.round(size)));
}

/** The size the selected tool draws with (`ToolSettings::size_of`). */
export function toolSize(state: UiState, tool: ToolId | null = state.tool): number {
  return clampSize((tool && state.sizes[tool]) ?? state.thickness);
}

function withSize(state: UiState, size: number): UiState {
  const clamped = clampSize(size);
  if (state.tool) return { ...state, sizes: { ...state.sizes, [state.tool]: clamped } };
  return { ...state, thickness: clamped };
}

export function reduce(state: UiState, action: Action): UiState {
  switch (action.type) {
    case "select_tool": {
      // Picking the selected tool again puts it down, as on the overlay
      const tool = action.tool === state.tool ? null : action.tool;
      return { ...state, tool, cropping: false, picker: { ...state.picker, eyedropper: false } };
    }
    case "set_size":
      return withSize(state, action.size);
    case "step_size":
      return withSize(state, toolSize(state) + action.by);
    case "set_options":
      return { ...state, options: { ...state.options, ...action.options } };
    case "set_color": {
      // Applied at once: there is no OK step
      const hue = action.hue ?? rgbToHsv(action.color, state.picker.hue).h;
      return { ...state, color: action.color, picker: { ...state.picker, hue } };
    }
    case "open_picker":
      return {
        ...state,
        picker: {
          ...state.picker,
          open: true,
          anchor: action.anchor ?? null,
          hue: rgbToHsv(state.color, state.picker.hue).h,
        },
      };
    case "close_picker":
      // Closing keeps the colour last picked; undo is what takes it back
      return { ...state, picker: { ...state.picker, open: false, eyedropper: false } };
    case "eyedropper":
      return { ...state, picker: { ...state.picker, eyedropper: action.on } };
    case "set_view":
      return { ...state, view: action.view };
    case "crop":
      return { ...state, cropping: action.on, tool: action.on ? null : state.tool };
  }
}
