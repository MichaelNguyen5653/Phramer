# Editor zoom, canvas expansion, and the capture-overlay editor hint

**Date:** 2026-09-11
**Status:** Approved for planning

## Summary

Three user-facing features, landing in two places:

1. **Editor zoom** — a magnifier mode plus `Ctrl+wheel`, zooming toward the
   cursor.
2. **Annotation outside the image** — the editable canvas grows to contain any
   annotation drawn past the image edge, filling the new space with a
   contrast-aware mat that is part of the export.
3. **Capture-overlay hint** — an unobtrusive pill advertising the key that
   opens the editor.

Features 1 and 2 are a single architectural change and are specified together.
Feature 3 is independent.

## Motivation

The standalone editor currently shows one image at one size. A capture larger
than the window can only be scrolled, never scaled down to see at once, and
detail work on a small region means squinting. Annotation is also hard-limited
to the image rectangle, so a caption or an arrow pointing at something cannot
sit in a margin beside the screenshot.

Separately, the editor is reachable from the capture overlay by a key
(`E` by default) that nothing on screen mentions, so most users never find it.

## Background: the coordinate contract being changed

`EditorCanvas` documents its own central invariant:

> The widget is sized to the image's device-independent size and is meant to
> live inside a scroll area, so widget coordinates and tool coordinates are the
> same thing and no transform is threaded through the tools.

Zoom breaks this by making widget pixels and image pixels differ in scale.
Canvas expansion breaks it by moving the origin. They are therefore one change,
not two, and must be designed together.

Note that the canvas already carries a device pixel ratio: `m_original` is
given the capture screen's ratio so annotations drawn at logical coordinates
land on the full-resolution output. Zoom is a *third* scale factor on top of
that, and must not be confused with it. The device pixel ratio stays entirely
inside the pixmap; zoom lives entirely in the widget-to-image mapping.

## Design

### Image space

Image space becomes the tool coordinate system. Its origin is the original
image's top-left corner, fixed forever at `(0, 0)`. **Negative coordinates are
legal** and denote positions above or to the left of the original image.

Pinning the origin to the image rather than to the canvas is the decision that
makes expansion cheap: when the canvas grows leftward, no existing object's
stored coordinates change. Only rendering and widget sizing translate.

Two new members on `EditorCanvas` carry the coordinate change (the mat
colour override in a later section adds a third):

| Member | Meaning |
|---|---|
| `qreal m_zoom` | Display scale, 0.1 to 8.0, default 1.0 |
| `QRect m_canvasRect` | The editable area in image space. Starts as `QRect(QPoint(0,0), imageSize)` |

The widget's size is `m_canvasRect.size() * m_zoom`, in device-independent
pixels.

### The mapping

Exactly two functions know that zoom exists:

```cpp
QPoint toImage(const QPoint& widgetPos) const;
QPoint fromImage(const QPoint& imagePos) const;
```

with `toImage(p) == m_canvasRect.topLeft() + p / m_zoom`.

Every site in `editorcanvas.cpp` that currently reads `event->pos()` and hands
it to a tool is routed through `toImage()`. There are roughly ten, all inside
the mouse handlers and `handleToolSignal`. Nothing in `src/tools/` changes.

`paintEvent` applies the inverse once:

```cpp
painter.scale(m_zoom, m_zoom);
painter.translate(-m_canvasRect.topLeft());
// ... existing drawing, unchanged
```

In-progress objects and the selection outline are drawn by the same painter, so
they follow automatically.

The header's invariant is restated as: *image coordinates are tool
coordinates*. It remains a single sentence, which is the point.

### Zoom interaction

- A magnifier action joins the editor toolbar's existing exclusive
  `m_toolGroup`, making magnifier mode mutually exclusive with drawing in the
  same way select mode already is.
- While magnifier mode is active, the plain wheel zooms at the cursor and drag
  pans the view.
- In any mode, `Ctrl+wheel` zooms at the cursor. `Ctrl+0` resets to 100%,
  `Ctrl+=` zooms in, `Ctrl+-` zooms out.
- Steps are geometric, a factor of 1.25 per notch, clamped to 10%–800%, with a
  snap to exactly 100% when a step would cross it.
- Zoom is anchored at the cursor: the image point under the pointer is recorded
  before the change and the scroll area's scroll bars are corrected after the
  widget resizes so that point stays put.
- The current zoom percentage is shown in the existing status bar.
- Zoom is per-canvas, so each image in the filmstrip keeps its own.
- `QScrollArea::setWidgetResizable(false)` stays as it is. The canvas resizes
  itself and the scroll area follows, which is why the existing structure
  absorbs zoom cheaply.

### Zoom shortcut

The magnifier binds to `Z`.

The capture shortcut table's single-letter defaults are P, D, A, S, R, C, M, B,
I, T, G, O and E. `Z` is unused, so a usable key exists and the action is bound
rather than left blank.

The magnifier is an editor toolbar action, **not** a `CaptureTool`. It gets no
`CaptureTool::Type` entry, no capture-overlay button, no SVG in the capture
button set, and no row in the capture shortcut table. It is bound directly on
its `QAction`. This keeps `CaptureTool::Type` — whose numbering is persisted in
user configs — untouched.

### Canvas expansion

`m_canvasRect` is recomputed from scratch whenever the object list changes:
after a commit, an undo or redo, a delete, or a move. The rule is the image
rectangle united with every object's `boundingRect()`, padded by 8 px in image
space so strokes are not flush against the edge, and never smaller than the
image rectangle.

Recomputing from scratch rather than growing incrementally is deliberate: it
makes the canvas shrink back on its own when the outlying annotation is undone
or deleted, with no separate shrink path to get wrong.

Drawing past the edge needs no new input handling. While a mouse button is
held, Qt's implicit mouse grab keeps delivering move events to the widget even
when the pointer leaves its geometry, and those events carry coordinates
outside the widget rectangle. The user drags past the edge, releases, and the
canvas expands on commit. Once expanded, strokes can also be *started* in the
new area.

`renderObjects()` changes by three lines at its head:

```cpp
QPixmap pixmap(m_canvasRect.size() * m_original.devicePixelRatio());
pixmap.setDevicePixelRatio(m_original.devicePixelRatio());
pixmap.fill(matColor());
QPainter base(&pixmap);
base.drawPixmap(-m_canvasRect.topLeft(), m_original);
```

followed by a `painter.translate(-m_canvasRect.topLeft())` before the existing
object loop, which is otherwise unchanged.

Because `rendered()` returns that pixmap, the mat is already included in every
consumer: save, copy, Save All and OCR all require no modification.

### Mat colour

The default is chosen for contrast against the image. The full 1-pixel perimeter of
`m_original` is sampled, its mean relative luminance computed, and the mat set
to `#F2F2F2` when the border is dark (luminance below 0.5) or `#1E1E1E` when it
is light. This is computed once when the image loads, not per frame.

A colour button beside the existing draw-colour control overrides it. The
override is per-canvas state, held on `EditorCanvas`, not a preference — it
describes one image, not the user's taste. **No new configuration key is
added for it.**

### Text tool under zoom

The text tool's editor is a real child `QWidget`, so `painter.scale()` does not
reach it. Its font point size is multiplied by `m_zoom` and it is positioned
with `fromImage()`.

This is an accepted, documented limitation: at extreme zoom the editor's glyph
metrics will not match the committed render pixel-for-pixel, because font
hinting is not linear in point size. The discrepancy is visible only while
typing and resolves on commit. The alternative considered and rejected was
forbidding zoom while a text box is open, which is a worse user experience for
a narrower problem.

Positioning must go through the existing anchor helpers rather than around
them. `TextTool::childWidgetOffset()` is box-relative and `EditorCanvas`
already feeds it `*m_activeTool->pos()`; that stays true, with `fromImage()`
applied to the result.

### Capture-overlay hint

The hint is **painted inside `CaptureWidget::paintEvent`**, in the same manner
as the existing selection-geometry pill. It is not a widget. This is the
central decision: a widget on the capture overlay would take mouse events on a
surface whose entire purpose is capturing drags, and would need z-order and
click-through handling that painting avoids entirely.

**Text.** `tr("Tip: press %1 to open the editor")`, with the key read live from
`ConfigHandler().shortcut("TYPE_OPEN_IN_EDITOR")` and rendered through
`QKeySequence(...).toString(QKeySequence::NativeText)`. If the user has cleared
that binding, the hint is not drawn at all, so it can never advertise a key
that does nothing.

**Appearance.** A rounded pill in the UI colour at roughly 75% alpha, with text
in black or white chosen by `ColorUtils::colorIsDark`, at the default font
size, on one line, with no icon graphic. The pill is sized to its text.

**Placement.** Candidate positions are tried in order:

1. Above the selection, with an 8 px gap
2. Below the selection, with an 8 px gap
3. Inside the selection, against its bottom edge

Above is tried first because the tool buttons default to below, so leading with
above avoids the common collision. A candidate is rejected if it falls outside
the active screen or intersects a tool button. The pill is centred
horizontally on the selection and clamped to the screen.

**Lifetime.** Drawn while a selection exists and no annotation has been started
in the current capture. A flag set on the first `startDrawing` suppresses it
for the remainder of that capture.

**Configuration.** `OPTION("showEditorHint", Bool(true))` in
`confighandler.cpp`, a matching `CONFIG_GETTER_SETTER` in `confighandler.h`,
and a checkbox in Settings → General reading "Show the editor keyboard tip".
The key is additive; per project convention it must never be removed once
shipped.

## Components and boundaries

| Unit | Responsibility | Depends on |
|---|---|---|
| `EditorCanvas` zoom/canvas state | Owns `m_zoom` and `m_canvasRect`; provides `toImage`/`fromImage` | Nothing new |
| Canvas geometry helpers | Compute canvas rect from objects; clamp and step zoom | Free functions, no widget |
| Mat colour helper | Border luminance sample to mat colour | `QPixmap` only |
| Editor toolbar magnifier | Mode toggle, `Ctrl` shortcuts, status-bar readout | `EditorCanvas` public API |
| Hint painter | Text, styling, candidate placement | `ConfigHandler`, `ButtonHandler` occupancy |

The geometry, mat-colour and hint-placement helpers are written as free
functions taking plain values so they can be tested without constructing a
widget.

## Testing

`tests/tools/` already holds geometry unit tests behind a `BUILD_TOOL_TESTS`
CMake option. A `tests/editor/` directory and `BUILD_EDITOR_TESTS` option
follow the same pattern, covering:

- `toImage`/`fromImage` round-trips across zoom levels and canvas offsets,
  including negative canvas origins
- Canvas rectangle growth and shrink for objects at negative coordinates and
  past the right and bottom edges, and that undoing an outlying object restores
  the previous rectangle
- Zoom stepping: clamping at both ends, and snapping to exactly 100%
- Mat colour selection for dark and light borders, including a borderline case
- Hint placement choosing above, below, or inside for selections at the top,
  middle and bottom of the screen, and rejecting a candidate that intersects a
  button rectangle

Interactive behaviour — wheel-zoom feel, cursor anchoring, dragging past the
edge — is verified by the user. This workstation does not run the application.

## Risks and limitations

- **Text editor fidelity under zoom**, described above. Accepted.
- **Zoom plus device pixel ratio.** Two scale factors are now in play. The
  device pixel ratio stays inside the pixmap and zoom stays inside the
  widget-to-image mapping; no code should combine them except the
  `renderObjects()` allocation, which multiplies the canvas size by the ratio
  and not by the zoom.
- **Memory at large canvases.** A greatly expanded canvas on a high-DPI capture
  allocates a correspondingly large pixmap on every `renderObjects()` call. The
  8 px pad is small and expansion is driven by actual annotations, so this is
  bounded by user action, but a pathological drag far off the image would
  allocate accordingly. Not mitigated in this version.
- **Hint and button collision** is handled by rejection, not by relayout. If
  every candidate is rejected the hint is simply not drawn that frame.

## Out of scope

- Zoom in the capture overlay. The overlay is a fullscreen, per-screen window
  whose geometry is fixed to its screen.
- Annotating outside the frame in the capture overlay. There is nothing to
  expand into.
- An explicit "expand canvas by N pixels" control. Expansion is driven by
  annotations, per the approved design.
- Persisting zoom or mat colour across editor sessions.
