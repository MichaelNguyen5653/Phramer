# Annotation delete fix + frosted blur — implementation plan

Date: 2026-09-21. Decisions: editor stays Qt (Electron rejected: ~2x install
size, duplicate tool implementations, Chromium patch cadence). Blur style:
secure "frosted glass".

## Part A — Backspace deletes the selected annotation on the capture screen

Confirmed by the user: the Delete key already works in both the editor and
the capture screen. Backspace works in the editor (`EditorCanvas::
keyPressEvent`) but not on the capture screen, because
`TYPE_DELETE_CURRENT_TOOL` is bound to `Delete` only on Windows
(confighandler.cpp ~262) and `Backspace` is on the reserved-shortcut list.

### Tasks

1. In `CaptureWidget::initShortcuts`, add a hard-wired
   `newShortcut(Qt::Key_Backspace, this, SLOT(deleteCurrentTool()))` next to
   the configured one. Do not change the stored default: existing INIs keep
   their saved value, and Backspace stays reserved, so it cannot collide with
   a user binding.
2. Guard: while a text box (`m_toolWidget`) has focus, Backspace must edit
   the text. A QShortcut on the capture widget could steal the key from the
   child editor. Either check `m_toolWidget` in the slot, or rely on the
   editor accepting `ShortcutOverride`. Verify by test.
3. With no object selected, Backspace does nothing (it must not close the
   capture or undo).
4. Discoverability: mention Del/Backspace in the help overlay text.

## Part B — Frosted-glass blur (secure)

### Why it looks bad now

- The secure mode samples only the 1px fringe around the rectangle, adds
  Gaussian sampling noise plus colour noise, and upscales a low-resolution
  grid with no smoothing. The result is a speckled, blocky smear.
- Bug: `weight_h = (qMin(x, width - x) / width) - (qMin(y, height - y) /
  height) + 0.5` uses integer division, so both terms are 0 and the weight is
  always 0.5. The intended edge-weighted blend never happens.
- Insecure mode: `QGraphicsBlurEffect` samples transparent pixels outside the
  rectangle, which gives a dark or faded halo at the edges.

### Design

The security rule is unchanged: **interior pixels are never read.**

1. Extract the four fringe lines, as now, in physical pixels.
2. Smooth each fringe line with a 1D Gaussian (sigma ≈ 8% of its length) so
   it carries only low-frequency colour and no edge detail.
3. Fill a low-resolution grid (~1/8 of the size) with a **Coons-patch
   (transfinite) interpolation** of the four smoothed edges. This gives a
   continuous gradient that matches the surroundings at every edge, so there
   is no visible seam.
4. Upscale with `Qt::SmoothTransformation` and add fine deterministic
   monochrome grain (±2–3 levels, fixed seed) so it reads as frosted glass
   rather than a flat gradient, and to avoid banding.
5. Optional finish: a 1px inner highlight at ~8% white, plus a small corner
   radius (4 logical px), drawn with painter state saved and restored.
6. `size()` controls the grain and smoothing strength instead of a block
   count.

Put the maths in a pure function so it can be tested without a painter:
`QImage frostedFill(const std::array<QImage,4>& fringes, QSize out,
quint32 seed)` in `src/tools/pixelate/frostedfill.{h,cpp}`. Add it to
`target_sources`: the CMake glob does not build new files.

### Tasks

1. Write tests in `tests/tools/` first:
   - **Security:** two source images that differ only inside the rectangle
     must give byte-identical output.
   - **Continuity:** output edge pixels are within ε of the smoothed fringe.
   - **Determinism:** same input gives the same output.
   - **HiDPI:** a DPR 1.25 pixmap gives output that covers the physical rect
     exactly.
2. Implement `frostedFill` and switch the secure branch of
   `PixelateTool::process` to it. Keep the tool type, config keys and icon
   (`insecurePixelate` stays).
3. Insecure branch: blur a padded copy (rectangle grown by 3σ, clamped to
   the image) and crop, so there is no dark halo. Low priority.
4. Performance: a full-screen 4K rectangle must stay interactive while
   dragging. `process()` runs on every repaint for every object, so cache the
   result keyed on (rect, size, fringe hash).
5. Check it in the overlay and the editor, with zoom, on light and dark
   backgrounds. Do this by manual screenshot review, done by the user; the
   agent does not launch phramer.

## Part C — Keep overlay annotations editable in the editor

Today "Open in Editor" sends `m_context.selectedScreenshotArea()`, a pixmap
with the annotations already baked in, so the editor receives flat pixels.

The ingredients already exist:

- `m_context.origScreenshot` is the clean screenshot (drawToolsData rebuilds
  from it).
- The editor uses the same `CaptureTool` and `CaptureToolObjects` types, and
  already keeps a clean original plus a live object list.
- `EditorCanvas::restoreObjects()` loads an object list.

### Tasks

1. `EditorWindow::addCapture(const QPixmap& clean, const CaptureToolObjects&
   objects)` overload → `EditorCanvas` constructor taking initial objects.
   The imported objects are the base state, and the undo stack starts empty.
2. In `~CaptureWidget`, when the request has `OPEN_IN_EDITOR`, pass the
   clean crop of `origScreenshot` plus deep copies (`tool->copy(canvas)`) of
   the objects, translated by `-selection.topLeft()` in logical pixels. The
   export stays in the destructor, so the export rule holds. Save, copy and
   pin still get the flattened pixmap.
3. Translation: `move()` shifts only the anchor for some tools. Verify
   two-point, path, text and circle-count objects all shift whole. Add
   `CaptureTool::translate(QPoint)` if any don't.
4. Circle count: call `restoreCircleCountState()` after the import so the
   next marker continues the numbering.
5. Pixelate and invert read source pixels through `sourceOrigin()`. Test
   that they sample the same area after the move to editor space.
6. HiDPI: a 125% capture opens with objects exactly where they were drawn.
   Add a unit test on the translation plus a manual check.
7. Drop any object in edit mode, or commit it first
   (`commitCurrentTool()`), so a half-typed text box isn't lost.

## Order and review

1. Part A (small, ships alone).
2. Part C.
3. Part B.

After each part: run `ecc:cpp-reviewer` on the diff, format with
clang-format 11 (the wheel binary), and build with
`-DUSE_PORTABLE_CONFIG=OFF -DCMAKE_BUILD_TYPE=Release` before any tag.
