// The editor window: title bar, toolbar, the image drawn by the engine, the
// options panel and the status bar.

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
  type PointerEvent as ReactPointerEvent,
  type WheelEvent as ReactWheelEvent,
} from "react";
import type { Backend, ExportAction, Notice } from "./backend";
import { sameRgb, type Rgb } from "./color";
import { ColorPicker } from "./ColorPicker";
import { EditorController } from "./controller";
import type { Engine, Point, Rect, Rendered } from "./engine";
import { Icon } from "./Icon";
import { OptionsPanel } from "./OptionsPanel";
import { initialState, reduce, toolSize, type EditorSettings } from "./state";
import {
  ACTIONS,
  FIXED_SHORTCUTS,
  TOOLS,
  dragTo,
  engineKind,
  isDrawing,
  matchesShortcut,
  objectSize,
  parseShortcut,
  shapeBetween,
  shortcutLabel,
  type ActionId,
  type Shortcut,
  type ToolId,
  type ToolOptions,
} from "./tools";
import { cssPerPixel, fit, pan, stepScale, toImage, toViewport, wheelSteps, zoomAt, centered } from "./zoom";

export interface EditorProps {
  engine: Engine;
  image: { width: number; height: number; rgba: Uint8Array };
  settings: EditorSettings;
  backend: Backend;
  /** Called once the image is on screen. */
  onReady?(): void;
}

/** What a pointer drag on the image is doing. */
type Drag =
  | { kind: "draw" }
  | { kind: "move"; start: Point }
  | { kind: "pan"; last: { x: number; y: number } }
  | { kind: "crop"; start: Point }
  | { kind: "eyedropper" };

const TOOLBAR_GROUPS: ActionId[][] = [
  ["undo", "redo"],
  ["crop"],
  ["zoom_out", "zoom_in", "fit", "actual_size"],
  ["copy", "copy_file", "save", "save_as", "pin", "ocr", "open_app"],
];

const NOTICE_MS = 3000;

function workspaceColor(): number {
  const value = getComputedStyle(document.documentElement).getPropertyValue("--bg").trim();
  const match = /^#?([0-9a-f]{6})$/i.exec(value);
  return match ? ((parseInt(match[1], 16) << 8) | 0xff) >>> 0 : 0xffffffff;
}

function floorPoint(at: { x: number; y: number }): Point {
  return { x: Math.floor(at.x), y: Math.floor(at.y) };
}

export function Editor({ engine, image, settings, backend, onReady }: EditorProps) {
  const controller = useMemo(
    () => EditorController.open(engine, image.width, image.height, image.rgba, settings.undo_limit),
    [engine, image, settings.undo_limit],
  );
  const version = useSyncExternalStore(controller.subscribe, controller.getVersion);
  const [state, dispatch] = useReducer(reduce, settings, initialState);
  const stateRef = useRef(state);
  stateRef.current = state;

  const viewportRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const textInputRef = useRef<HTMLTextAreaElement>(null);
  const rendered = useRef<Rendered | null>(null);
  const [placed, setPlaced] = useState<Rect | null>(null);
  const drag = useRef<Drag | null>(null);
  const spaceHeld = useRef(false);
  const colorBefore = useRef<Rgb>(state.color);
  const [cropRect, setCropRect] = useState<Rect | null>(null);
  const [cursor, setCursor] = useState<Point | null>(null);
  const [notices, setNotices] = useState<(Notice & { id: number })[]>([]);
  const [confirmClose, setConfirmClose] = useState(false);
  const [workspace, setWorkspace] = useState(workspaceColor);
  const [pickerPosition, setPickerPosition] = useState({ left: 0, top: 0 });
  const dpr = window.devicePixelRatio || 1;

  const shortcuts = useMemo(() => {
    const parsed: { shortcut: Shortcut; tool?: ToolId; action?: ActionId }[] = [];
    for (const tool of TOOLS) {
      const shortcut = tool.shortcut && parseShortcut(settings.shortcuts[tool.shortcut] ?? "");
      if (shortcut) parsed.push({ shortcut, tool: tool.id });
    }
    for (const action of ACTIONS) {
      const shortcut = action.shortcut && parseShortcut(settings.shortcuts[action.shortcut] ?? "");
      if (shortcut) parsed.push({ shortcut, action: action.id });
    }
    const extra: [string | undefined, ActionId][] = [
      [settings.shortcuts.TYPE_DELETE_CURRENT_TOOL, "delete"],
      [settings.shortcuts.TYPE_COMMIT_CURRENT_TOOL, "commit"],
      ...FIXED_SHORTCUTS,
    ];
    for (const [text, action] of extra) {
      const shortcut = text ? parseShortcut(text) : null;
      if (shortcut) parsed.push({ shortcut, action });
    }
    return parsed;
  }, [settings.shortcuts]);

  const labelOf = useCallback(
    (id: ToolId | ActionId) => {
      const found = shortcuts.find((entry) => entry.tool === id || entry.action === id);
      return found ? ` (${shortcutLabel(found.shortcut)})` : "";
    },
    [shortcuts],
  );

  // --- Showing the image ---------------------------------------------------------

  useEffect(() => {
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => setWorkspace(workspaceColor());
    scheme.addEventListener("change", update);
    return () => scheme.removeEventListener("change", update);
  }, []);

  useLayoutEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const image = controller.render(workspace);
    rendered.current = image;
    if (canvas.width !== image.width) canvas.width = image.width;
    if (canvas.height !== image.height) canvas.height = image.height;
    canvas.getContext("2d")?.putImageData(new ImageData(image.rgba, image.width, image.height), 0, 0);
    setPlaced((current) =>
      current && current.x === image.x && current.y === image.y && current.width === image.width && current.height === image.height
        ? current
        : { x: image.x, y: image.y, width: image.width, height: image.height },
    );
  }, [controller, version, workspace]);

  const viewportSize = () => {
    const box = viewportRef.current?.getBoundingClientRect();
    return { width: box?.width ?? 800, height: box?.height ?? 600 };
  };
  const session = controller.session;

  // Fitted (never past 100%) when it opens, and after a crop
  useLayoutEffect(() => {
    dispatch({ type: "set_view", view: fit(session, viewportSize(), dpr, false) });
  }, [session, dpr]);

  const readied = useRef(false);
  useEffect(() => {
    if (readied.current || !placed) return;
    readied.current = true;
    requestAnimationFrame(() => onReady?.());
  }, [placed, onReady]);

  const viewportPoint = (event: { clientX: number; clientY: number }) => {
    const box = viewportRef.current?.getBoundingClientRect();
    return { x: event.clientX - (box?.left ?? 0), y: event.clientY - (box?.top ?? 0) };
  };
  const imagePoint = (event: { clientX: number; clientY: number }) =>
    toImage(stateRef.current.view, viewportPoint(event), dpr);

  // --- Notices and settings ---------------------------------------------------------

  const notify = useCallback((list: Notice[]) => {
    for (const notice of list) {
      const id = Math.random();
      setNotices((current) => [...current, { ...notice, id }]);
      setTimeout(() => setNotices((current) => current.filter((entry) => entry.id !== id)), NOTICE_MS);
    }
  }, []);

  const persist = useCallback(
    (change: Parameters<Backend["set"]>[0]) => {
      backend.set(change).catch((error: unknown) => notify([{ text: `Cannot save the setting: ${String(error)}`, error: true }]));
    },
    [backend, notify],
  );

  const setColorLive = useCallback(
    (color: Rgb, hue?: number) => {
      dispatch({ type: "set_color", color, hue });
      controller.previewColor(color);
    },
    [controller],
  );

  const commitColor = useCallback(
    (color: Rgb) => {
      const before = colorBefore.current;
      colorBefore.current = color;
      controller.commitColor(before, color);
      if (!sameRgb(before, color)) persist({ key: "draw_color", color: [color.r, color.g, color.b] });
    },
    [controller, persist],
  );

  const commitSize = useCallback(
    (size: number) => {
      dispatch({ type: "set_size", size });
      const current = stateRef.current;
      const clamped = Math.min(50, Math.max(1, Math.round(size)));
      const object = controller.selectedObject();
      const kind = object?.shape.tool ?? (current.tool ? engineKind(current.tool) : "line");
      controller.commitSize(objectSize(kind, clamped, dpr));
      if (current.tool) persist({ key: "tool_size", tool: current.tool, size: clamped });
    },
    [controller, persist, dpr],
  );

  const setOptions = useCallback(
    (options: Partial<ToolOptions>) => {
      dispatch({ type: "set_options", options });
      const next = { ...stateRef.current.options, ...options };
      if ("shapeKind" in options || "shapeStyle" in options) persist({ key: "shape", kind: next.shapeKind, style: next.shapeStyle });
      if ("arrowHead" in options || "reverseArrow" in options) persist({ key: "arrow", head: next.arrowHead, reversed: next.reverseArrow });
      if ("rectangleStyle" in options) persist({ key: "rectangle_style", style: next.rectangleStyle });
      if ("highlightOutline" in options) persist({ key: "highlight_outline", width: next.highlightOutline });
      if ("redact" in options) persist({ key: "redact", style: next.redact });
      const text = controller.text;
      if (text && ("textAlign" in options || "underline" in options || "strikeout" in options)) {
        controller.text = {
          ...text,
          align: next.textAlign,
          font: { ...text.font, underline: next.underline, strikeout: next.strikeout },
        };
        controller.changed();
      }
    },
    [controller, persist],
  );

  // --- Actions ----------------------------------------------------------------------

  const requestClose = useCallback(() => {
    controller.commitText();
    if (controller.dirty) setConfirmClose(true);
    else void backend.destroy();
  }, [backend, controller]);

  useEffect(() => {
    const unlisten = backend.onCloseRequested(() => {
      controller.commitText();
      if (!controller.dirty) return true;
      setConfirmClose(true);
      return false;
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [backend, controller]);

  const exportAs = useCallback(
    async (action: ExportAction) => {
      const image = controller.flatten(workspace);
      try {
        const result = await backend.exportImage(action, image);
        notify(result);
        if (["save", "save_as", "copy_file"].includes(action) && result.length > 0 && !result.some((notice) => notice.error)) {
          controller.markSaved();
        }
      } catch (error) {
        notify([{ text: String(error), error: true }]);
      }
    },
    [backend, controller, notify, workspace],
  );

  const applyCrop = useCallback(() => {
    if (cropRect && controller.crop(cropRect, workspace)) {
      setCropRect(null);
      dispatch({ type: "crop", on: false });
    }
  }, [controller, cropRect, workspace]);

  const run = useCallback(
    (action: ActionId) => {
      const current = stateRef.current;
      switch (action) {
        case "undo":
        case "redo": {
          const result = action === "undo" ? controller.undo() : controller.redo();
          if (result.color) {
            dispatch({ type: "set_color", color: result.color });
            colorBefore.current = result.color;
            persist({ key: "draw_color", color: [result.color.r, result.color.g, result.color.b] });
          }
          break;
        }
        case "copy":
        case "copy_file":
        case "save":
        case "save_as":
        case "pin":
        case "ocr":
        case "open_app":
          void exportAs(action);
          break;
        case "close":
          requestClose();
          break;
        case "size_increase":
        case "size_decrease":
          commitSize(toolSize(current) + (action === "size_increase" ? 1 : -1));
          break;
        case "crop":
          if (current.cropping && cropRect) applyCrop();
          else {
            controller.commitText();
            setCropRect(null);
            dispatch({ type: "crop", on: !current.cropping });
          }
          break;
        case "zoom_in":
        case "zoom_out": {
          const size = viewportSize();
          const scale = stepScale(current.view.scale, action === "zoom_in" ? 1 : -1);
          dispatch({ type: "set_view", view: zoomAt(current.view, scale, { x: size.width / 2, y: size.height / 2 }, dpr) });
          break;
        }
        case "fit":
          dispatch({ type: "set_view", view: fit(controller.session, viewportSize(), dpr, true) });
          break;
        case "actual_size":
          dispatch({ type: "set_view", view: centered(1, controller.session, viewportSize(), dpr) });
          break;
        case "delete":
          controller.deleteSelected();
          break;
        case "commit":
          controller.commitText();
          break;
      }
    },
    [applyCrop, commitSize, controller, cropRect, dpr, exportAs, persist, requestClose],
  );

  const selectTool = useCallback(
    (tool: ToolId | null) => {
      controller.commitText();
      if (isDrawing(tool)) controller.select(null);
      dispatch({ type: "select_tool", tool });
    },
    [controller],
  );

  // --- Picker ---------------------------------------------------------------------------

  const openPicker = useCallback(
    (at: { left: number; top: number }, anchor: { x: number; y: number } | null) => {
      colorBefore.current = stateRef.current.color;
      const width = 248;
      const height = 330;
      setPickerPosition({
        left: Math.max(8, Math.min(at.left, window.innerWidth - width - 8)),
        top: Math.max(8, Math.min(at.top, window.innerHeight - height - 8)),
      });
      dispatch({ type: "open_picker", anchor });
    },
    [],
  );

  const closePicker = useCallback(() => {
    // Closing keeps what was picked
    if (stateRef.current.picker.open) dispatch({ type: "close_picker" });
  }, []);

  // A press anywhere else closes it, keeping the colour
  useEffect(() => {
    if (!state.picker.open) return;
    const away = (event: PointerEvent) => {
      const target = event.target as Element | null;
      if (target?.closest(".picker, .color-button, .viewport")) return;
      closePicker();
    };
    window.addEventListener("pointerdown", away);
    return () => window.removeEventListener("pointerdown", away);
  }, [state.picker.open, closePicker]);

  // --- Pointer ----------------------------------------------------------------------------

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = stateRef.current;
    const at = imagePoint(event);
    const point = floorPoint(at);
    viewportRef.current?.focus();
    if (current.picker.eyedropper && event.button === 0) {
      drag.current = { kind: "eyedropper" };
      event.currentTarget.setPointerCapture(event.pointerId);
      const sample = rendered.current && EditorController.sample(rendered.current, at);
      if (sample) setColorLive(sample);
      return;
    }
    if (current.picker.open) closePicker();
    if (event.button === 1 || (event.button === 0 && spaceHeld.current)) {
      drag.current = { kind: "pan", last: viewportPoint(event) };
      event.currentTarget.setPointerCapture(event.pointerId);
      return;
    }
    if (event.button === 2) {
      // The overlay's right-click colour wheel: here, the picker at the pointer
      const hit = controller.text ? null : controller.hitTest(point);
      if (hit !== null && !isDrawing(current.tool)) controller.select(hit);
      openPicker({ left: event.clientX, top: event.clientY }, viewportPoint(event));
      return;
    }
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    if (current.cropping) {
      drag.current = { kind: "crop", start: point };
      setCropRect({ x: point.x, y: point.y, width: 0, height: 0 });
      return;
    }
    if (controller.text) {
      const area = controller.session.annotationBounds({
        color: controller.text.color,
        size: controller.text.size,
        shape: controller.textShape(controller.text, controller.text.text || "X"),
      });
      const inside = area && point.x >= area.x && point.y >= area.y && point.x < area.x + area.width && point.y < area.y + area.height;
      if (!inside) controller.commitText();
      else {
        // Keep the keys going to the text: no mousedown to move the focus
        event.preventDefault();
        textInputRef.current?.focus();
      }
      return;
    }
    const tool = current.tool;
    const color = { ...current.color, a: 255 };
    if (tool === "text") {
      const size = objectSize("text", toolSize(current), dpr);
      const font = { underline: current.options.underline, strikeout: current.options.strikeout };
      controller.startText(point, color, size, font, current.options.textAlign);
      // Typed keys go to the hidden text field from the first one on
      event.preventDefault();
      textInputRef.current?.focus();
      return;
    }
    if (isDrawing(tool)) {
      const shape = shapeBetween(tool, point, point, current.options, dpr, controller.session.state().next_counter);
      if (!shape) return;
      controller.startDraft(color, objectSize(shape.tool, toolSize(current), dpr), shape);
      drag.current = { kind: "draw" };
      return;
    }
    const hit = controller.hitTest(point);
    if (hit !== null) {
      controller.select(hit);
      drag.current = { kind: "move", start: point };
    } else {
      if (controller.selected !== null) controller.select(null);
      // Dragging the empty canvas pans, as 15.0.0's Select mode did
      drag.current = { kind: "pan", last: viewportPoint(event) };
    }
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const at = imagePoint(event);
    const point = floorPoint(at);
    setCursor(point);
    const current = drag.current;
    if (!current) {
      if (stateRef.current.picker.eyedropper && rendered.current) {
        const sample = EditorController.sample(rendered.current, at);
        if (sample) setColorLive(sample);
      }
      return;
    }
    switch (current.kind) {
      case "eyedropper": {
        const sample = rendered.current && EditorController.sample(rendered.current, at);
        if (sample) setColorLive(sample);
        break;
      }
      case "pan": {
        const here = viewportPoint(event);
        dispatch({ type: "set_view", view: pan(stateRef.current.view, here.x - current.last.x, here.y - current.last.y) });
        drag.current = { kind: "pan", last: here };
        break;
      }
      case "draw": {
        const draft = controller.draft;
        const tool = stateRef.current.tool;
        if (!draft || !tool) break;
        let end = point;
        if (event.shiftKey && "start" in draft.shape) {
          end = controller.session.constrain(draft.shape.start, point, engineKind(tool));
        }
        controller.updateDraft(dragTo(draft.shape, end));
        break;
      }
      case "move":
        controller.moveBy(point.x - current.start.x, point.y - current.start.y);
        break;
      case "crop":
        setCropRect({
          x: Math.min(current.start.x, point.x),
          y: Math.min(current.start.y, point.y),
          width: Math.abs(point.x - current.start.x),
          height: Math.abs(point.y - current.start.y),
        });
        break;
    }
  };

  const onPointerUp = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    drag.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (!current) return;
    if (current.kind === "draw") controller.finishDraft();
    if (current.kind === "move") controller.finishMove();
    if (current.kind === "eyedropper") {
      commitColor(stateRef.current.color);
      dispatch({ type: "eyedropper", on: false });
    }
  };

  const onWheel = (event: ReactWheelEvent<HTMLDivElement>) => {
    const current = stateRef.current;
    if (event.ctrlKey) {
      const steps = wheelSteps(event.deltaY);
      dispatch({ type: "set_view", view: zoomAt(current.view, stepScale(current.view.scale, steps), viewportPoint(event), dpr) });
    } else if (event.altKey) {
      const steps = wheelSteps(event.deltaY);
      if (steps !== 0) commitSize(toolSize(current) + Math.sign(steps));
    } else if (event.shiftKey) {
      dispatch({ type: "set_view", view: pan(current.view, -(event.deltaY || event.deltaX), 0) });
    } else {
      dispatch({ type: "set_view", view: pan(current.view, -event.deltaX, -event.deltaY) });
    }
  };

  // Ctrl+wheel must not zoom the page itself
  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const stop = (event: WheelEvent) => {
      if (event.ctrlKey) event.preventDefault();
    };
    viewport.addEventListener("wheel", stop, { passive: false });
    return () => viewport.removeEventListener("wheel", stop);
  }, []);

  // --- Keys ---------------------------------------------------------------------------------

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const current = stateRef.current;
      const target = event.target as HTMLElement | null;
      const typingField = target && target !== textInputRef.current && (target.tagName === "INPUT" || target.tagName === "TEXTAREA");
      if (controller.text) {
        if (event.key === "Escape" || (event.key === "Enter" && event.ctrlKey)) {
          event.preventDefault();
          controller.commitText();
          return;
        }
        if (event.key === "Enter") {
          event.preventDefault();
          controller.typeText("\n");
          return;
        }
        const keys: Record<string, "backspace" | "delete" | "left" | "right" | "home" | "end"> = {
          Backspace: "backspace",
          Delete: "delete",
          ArrowLeft: "left",
          ArrowRight: "right",
          Home: "home",
          End: "end",
        };
        if (keys[event.key]) {
          event.preventDefault();
          controller.textKey(keys[event.key]);
        }
        // Characters arrive through the hidden text field
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        if (current.picker.open) closePicker();
        else if (current.cropping) {
          setCropRect(null);
          dispatch({ type: "crop", on: false });
        } else if (current.tool) selectTool(null);
        else if (controller.selected !== null) controller.select(null);
        if (typingField) target.blur();
        return;
      }
      if (typingField) return;
      if (event.key === " " && !event.repeat) {
        spaceHeld.current = true;
        event.preventDefault();
        return;
      }
      if (event.key === "Enter" && current.cropping) {
        event.preventDefault();
        applyCrop();
        return;
      }
      const found = shortcuts.find((entry) => matchesShortcut(entry.shortcut, event));
      if (!found) return;
      event.preventDefault();
      if (found.tool) selectTool(found.tool);
      else if (found.action) run(found.action);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === " ") spaceHeld.current = false;
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, [applyCrop, closePicker, controller, run, selectTool, shortcuts]);

  // --- Drawing the chrome ----------------------------------------------------------------------

  const view = state.view;
  const unit = cssPerPixel(view.scale, dpr);
  const canvasStyle = placed
    ? {
        transform: `translate(${view.x + placed.x * unit}px, ${view.y + placed.y * unit}px) scale(${unit})`,
        imageRendering: unit >= 1 ? ("pixelated" as const) : ("auto" as const),
      }
    : { opacity: 0 };
  const box = (rect: Rect | null) => {
    if (!rect) return undefined;
    const origin = toViewport(view, rect, dpr);
    return {
      transform: `translate(${origin.x}px, ${origin.y}px)`,
      width: rect.width * unit,
      height: rect.height * unit,
    };
  };
  const selectedBox = controller.selected !== null ? box(controller.selectedBounds()) : undefined;
  const caret = controller.caret();
  const caretStyle = caret
    ? {
        transform: `translate(${toViewport(view, caret.at, dpr).x}px, ${toViewport(view, caret.at, dpr).y}px)`,
        height: caret.height * unit,
      }
    : undefined;
  const engineState = controller.session.state();
  const selected = controller.selectedObject();
  const size = toolSize(state);
  const cursorClass = state.picker.eyedropper
    ? "eyedropper"
    : state.cropping || isDrawing(state.tool)
      ? state.tool === "text"
        ? "text"
        : "crosshair"
      : "grab";

  return (
    <div className="editor" data-testid="editor">
      <header className="titlebar" data-tauri-drag-region>
        <span className="title" data-tauri-drag-region>
          Phramer Editor
          <span className="title-detail" data-tauri-drag-region>
            {controller.session.width} × {controller.session.height}
            {controller.dirty ? " · edited" : ""}
          </span>
        </span>
        <div className="window-buttons">
          <button type="button" aria-label="Minimize" title="Minimize" onClick={() => void backend.minimize()}>
            <Icon name="minus" />
          </button>
          <button type="button" aria-label="Maximize" title="Maximize" onClick={() => void backend.toggleMaximize()}>
            <Icon name="square" />
          </button>
          <button type="button" className="close" aria-label="Close" title={`Close${labelOf("close")}`} onClick={requestClose}>
            <Icon name="x" />
          </button>
        </div>
      </header>

      <nav className="toolbar" aria-label="Tools" data-testid="toolbar">
        <div className="group" role="toolbar" aria-label="Drawing tools">
          {TOOLS.map((tool) => (
            <button
              key={tool.id}
              type="button"
              className={`tool${state.tool === tool.id ? " active" : ""}`}
              aria-pressed={state.tool === tool.id}
              aria-label={tool.name}
              title={`${tool.description}${labelOf(tool.id)}`}
              data-testid={`tool-${tool.id}`}
              onClick={() => selectTool(tool.id)}
            >
              <Icon name={tool.icon} />
            </button>
          ))}
        </div>
        {TOOLBAR_GROUPS.map((group, index) => (
          <div className="group" key={index}>
            {group.map((id) => {
              const action = ACTIONS.find((entry) => entry.id === id);
              if (!action) return null;
              const disabled = (id === "undo" && !controller.canUndo) || (id === "redo" && !controller.canRedo);
              const active = id === "crop" && state.cropping;
              return (
                <button
                  key={id}
                  type="button"
                  className={`tool${active ? " active" : ""}`}
                  aria-label={action.name}
                  aria-pressed={id === "crop" ? active : undefined}
                  title={`${action.description}${labelOf(id)}`}
                  disabled={disabled}
                  data-testid={`action-${id}`}
                  onClick={() => run(id)}
                >
                  <Icon name={action.icon} />
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <div className="workspace">
        <div
          ref={viewportRef}
          className={`viewport cursor-${cursorClass}`}
          data-testid="viewport"
          tabIndex={-1}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onPointerLeave={() => setCursor(null)}
          onWheel={onWheel}
          onContextMenu={(event) => event.preventDefault()}
        >
          <canvas ref={canvasRef} className="image" data-testid="canvas" style={canvasStyle} />
          {selectedBox && <div className="selection-outline" data-testid="selection" style={selectedBox} />}
          {caretStyle && <div className="caret" style={caretStyle} />}
          {state.cropping && cropRect && <div className="crop-rect" data-testid="crop-rect" style={box(cropRect)} />}
          {state.cropping && (
            <div className="hint" role="status">
              Drag the area to keep, then press Enter or Crop. Esc cancels.
            </div>
          )}
          <textarea
            ref={textInputRef}
            className="text-input"
            aria-label="Text"
            tabIndex={-1}
            value=""
            onChange={(event) => controller.typeText(event.target.value)}
          />
          {confirmClose && (
            <div className="confirm" role="alertdialog" aria-label="Unsaved changes">
              <span>This image has changes that are not saved.</span>
              <button type="button" className="text-button" onClick={() => setConfirmClose(false)}>
                Keep editing
              </button>
              <button type="button" className="text-button danger" onClick={() => void backend.destroy()}>
                Close without saving
              </button>
            </div>
          )}
        </div>
        <OptionsPanel
          tool={state.tool}
          color={state.color}
          size={selected && !state.tool ? Math.max(1, selected.size) : size}
          options={state.options}
          selected={selected}
          nextCounter={engineState.next_counter}
          pickerOpen={state.picker.open}
          onColorButton={(button) => {
            if (state.picker.open) closePicker();
            else {
              const rect = button.getBoundingClientRect();
              openPicker({ left: rect.right - 248, top: rect.bottom + 4 }, null);
            }
          }}
          onSizePreview={(next) => {
            dispatch({ type: "set_size", size: next });
            const kind = selected?.shape.tool;
            if (kind) controller.previewSize(objectSize(kind, next, dpr));
          }}
          onSize={commitSize}
          onOptions={setOptions}
          onNextCounter={(next) => {
            controller.session.setNextCounter(next);
            controller.changed();
          }}
          onDelete={() => controller.deleteSelected()}
        />
      </div>

      <footer className="statusbar" data-testid="statusbar">
        <span>
          {controller.session.width} × {controller.session.height} px
        </span>
        <span data-testid="zoom">{Math.round(view.scale * 100)}%</span>
        <span className="mono">{cursor ? `${cursor.x}, ${cursor.y}` : ""}</span>
        <span className="notices" aria-live="polite">
          {notices.map((notice) => (
            <span key={notice.id} className={`notice${notice.error ? " error" : ""}`}>
              {notice.text}
            </span>
          ))}
        </span>
      </footer>

      {state.picker.open && (
        <ColorPicker
          color={state.color}
          hue={state.picker.hue}
          palette={state.palette}
          eyedropper={state.picker.eyedropper}
          position={pickerPosition}
          onPick={setColorLive}
          onCommit={commitColor}
          onEyedropper={(on) => dispatch({ type: "eyedropper", on })}
        />
      )}
    </div>
  );
}
