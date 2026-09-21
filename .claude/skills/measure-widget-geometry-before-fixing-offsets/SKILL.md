---
name: measure-widget-geometry-before-fixing-offsets
description: "Use when a UI element lands at the wrong offset and the fix needs a framework's internal inset — build a throwaway probe to measure it rather than guessing or hardcoding a constant"
metadata:
  origin: auto-extracted
---

# Measure Widget Geometry Before Fixing Offsets

**Extracted:** 2026-09-11
**Context:** Qt widget positioning bugs in the Phramer capture and editor
canvases, where a live editor widget and its committed rendering must agree
on where glyphs land.

## Problem

"It's a few pixels off" bugs get fixed by nudging a magic number. The nudge
holds for the one font size, DPI and style it was tuned against and drifts
everywhere else, because the real inset is framework-owned — frame width,
document margin, viewport margins — and varies by style and Qt version.

The failure is worse when two code paths render the same content: a live
`QTextEdit` editor and the `QPainter` call that draws the committed object.
Each accumulates its own constants, they agree by coincidence, and the text
jumps the moment the edit is committed.

## Solution

Compile a minimal standalone program against the same Qt version, put the
real widget in it with the real stylesheet applied, and print the numbers.
Check them *before* `show()` and polish as well, since production code often
reads geometry early — if the value is only valid after polish, the fix needs
an `ensurePolished()` the probe will reveal.

Then express the fix in terms of the framework accessor, not the measured
literal. The measurement belongs in the commit message and the comment; the
code reads the accessor.

## Example

A `QTextEdit`'s first glyph sits at `frameWidth() + documentMargin()`.

```cpp
fprintf(stderr, "frameWidth=%d docMargin=%.1f cursorRect=(%d,%d %dx%d)\n",
        w.frameWidth(), w.document()->documentMargin(),
        cr.x(), cr.y(), cr.width(), cr.height());
```

Printed 2 + 4 = **(6, 6)**, constant across 9/11/16/28 pt, and already correct
before `show()`. The caret height tracked `QFontMetrics::height()` exactly, so
centring a line on a click is `- height()/2`. The shipped fix reads
`frameWidth() + qRound(document()->documentMargin())` at runtime.

Build the probe **outside** `%TEMP%` — MSBuild refuses to build there and
reports it as a broken compiler.

## When to Use

- Any "renders N px off" or "lands below the cursor" report
- Two code paths must agree on a position — a live editor widget versus its
  committed `process(QPainter&)` render
- Before hardcoding any pixel constant you cannot cite a source for
- When a placement bug gets worse as the font or tool size grows, which points
  at a missing font-metric term rather than a wrong constant
