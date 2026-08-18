# OCR Accuracy and Consistency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the OCR tool return text reliably, by reading exact text from the source window where possible and, where not, by running several scored recognition passes over tiled images instead of one clamped pass.

**Architecture:** `OcrWorker` becomes a two-stage chain — an exact UI Automation read first, then a scored recognition pipeline. The pipeline is a new pure module that takes recognition as a `std::function` callback, so its pass-selection logic is unit-testable with a fake engine, with no COM, no WinRT and no Windows dependency. Tiling replaces the scale clamp that currently prevents large captures from ever being upscaled.

**Tech Stack:** C++20, Qt 6.9.3 (Core, Gui, Test), CMake with the Visual Studio 18 2026 generator, QTest, Windows.Media.Ocr via C++/WinRT, UI Automation via COM.

**Spec:** [docs/superpowers/specs/2026-08-18-ocr-accuracy-design.md](../specs/2026-08-18-ocr-accuracy-design.md)

## Global Constraints

- **Format with clang-format 11 only.** The working binary is `C:\Users\DTN2\AppData\Local\Programs\Python\Python314\Lib\site-packages\clang_format\data\bin\clang-format.exe`, and it needs `C:\Users\DTN2\AppData\Local\Programs\Git\mingw64\bin` prepended to `PATH` or it exits silently with 0xC0000135. Confirm `--version` prints `clang-format version 11.1.0` before trusting a run.
- **Never launch `phramer.exe` or exercise any screen-capture path.** This is a corporate workstation with security monitoring. Building is fine; running the app is not. Runtime verification is the user's, and is listed at the end of Task 7 as a handoff, not as a step to perform.
- `ocr_tests` is the only thing that may be executed.
- Every new source file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`.
- Use `&Class::method` connect syntax. Wrap user-facing strings in `tr()`.
- Comment the constraint, not the mechanics — why a line must exist, not what it does.
- Never remove a shipped `ConfigHandler` key. This work adds no keys at all.
- New portable sources (`ocrpipeline`, `ocrtiling`, `ocrscore`) go in the **unguarded** `target_sources` block of `src/tools/CMakeLists.txt`, not the `if (WIN32)` block, because the test binary compiles them on any platform.
- Build directory is already configured with `BUILD_OCR_TESTS=ON`. Build with:
  `cmake --build build --config RelWithDebInfo --target ocr_tests`
  Test with:
  `ctest --test-dir build -C RelWithDebInfo -R ocr_tests --output-on-failure`
- Branch: `feature/ocr-accuracy` (already created, spec already committed as `a7059b19`).

---

### Task 1: Result scoring

Windows.Media.Ocr reports no confidence value, so the only way to tell a good pass from a bad one is how much text came back and how plausibly it is shaped. Everything later in the plan depends on this comparison existing.

**Files:**
- Create: `src/tools/ocr/ocrscore.h`
- Create: `src/tools/ocr/ocrscore.cpp`
- Modify: `src/tools/CMakeLists.txt` (unguarded `target_sources` block, around line 66)
- Modify: `tests/ocr/CMakeLists.txt` (the `add_executable` source list)
- Test: `tests/ocr/tst_ocr.cpp`

**Interfaces:**
- Consumes: `OcrResult`, `OcrLine`, `OcrWord` from `tools/ocr/ocrengine.h`
- Produces: `int ocrScoreResult(const OcrResult& result)`

- [ ] **Step 1: Add the new files to both CMake source lists**

In `src/tools/CMakeLists.txt`, the unguarded OCR block currently reads:

```cmake
target_sources(
  flameshot
  PRIVATE ocr/ocrengine.h
          ocr/ocrengine.cpp
          ocr/nullocrengine.h
          ocr/ocrpreprocess.h
          ocr/ocrpreprocess.cpp
          ocr/ocrlayout.h
          ocr/ocrlayout.cpp)
```

Append two lines so it ends:

```cmake
          ocr/ocrlayout.h
          ocr/ocrlayout.cpp
          ocr/ocrscore.h
          ocr/ocrscore.cpp)
```

In `tests/ocr/CMakeLists.txt`, extend the executable's sources:

```cmake
add_executable(
  ocr_tests
  tst_ocr.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrpreprocess.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrlayout.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrscore.cpp)
```

- [ ] **Step 2: Write the failing tests**

Add to `tests/ocr/tst_ocr.cpp`. First extend the anonymous namespace helpers at the top of the file, after `textsOf()`:

```cpp
OcrLine makeWordLine(const QStringList& words, qreal top)
{
    OcrLine line;
    line.text = words.join(QLatin1Char(' '));
    qreal x = 0;
    for (const QString& word : words) {
        OcrWord w;
        w.text = word;
        // 10px per character is arbitrary but consistent; only the text
        // lengths matter to scoring
        w.boundingBox = QRectF(x, top, word.size() * 10.0, 20.0);
        x += word.size() * 10.0 + 10.0;
        line.words.append(w);
    }
    line.boundingBox = QRectF(0, top, x, 20.0);
    return line;
}

OcrResult okResult(const QVector<OcrLine>& lines)
{
    OcrResult result;
    result.lines = lines;
    result.status = OcrResult::Status::Ok;
    return result;
}
```

Then add these test slots, in a new `// --- ocrScoreResult ---` section after the `ocrScaleImage` section:

```cpp
    // --- ocrScoreResult --------------------------------------------------

    void moreRecognizedTextScoresHigher()
    {
        const OcrResult rich = okResult(
          { makeWordLine({ QStringLiteral("hello"), QStringLiteral("world") },
                         0) });
        const OcrResult sparse =
          okResult({ makeWordLine({ QStringLiteral("hi") }, 0) });

        QVERIFY(ocrScoreResult(rich) > ocrScoreResult(sparse));
    }

    void scatteredPunctuationLosesToRealText()
    {
        // A badly scaled pass comes back as stray marks spread over many
        // lines. Line count alone would let that outscore a clean result.
        QVector<OcrLine> junk;
        for (int i = 0; i < 8; ++i) {
            junk.append(makeWordLine({ QStringLiteral(".") }, i * 30.0));
        }
        const OcrResult clean = okResult(
          { makeWordLine({ QStringLiteral("hello"), QStringLiteral("world") },
                         0) });

        QVERIFY(ocrScoreResult(clean) > ocrScoreResult(okResult(junk)));
    }

    void aFailedResultScoresNothing()
    {
        OcrResult failed = okResult(
          { makeWordLine({ QStringLiteral("ignored") }, 0) });
        failed.status = OcrResult::Status::EngineError;

        QCOMPARE(ocrScoreResult(failed), 0);
    }

    void scoresLinesThatHaveNoWordBoxes()
    {
        // Line text without word boxes still counts; an engine may report it
        OcrLine line;
        line.text = QStringLiteral("abcd");
        line.boundingBox = QRectF(0, 0, 40, 20);

        QCOMPARE(ocrScoreResult(okResult({ line })), 6);
    }
```

Add the include at the top of `tst_ocr.cpp`, keeping the existing alphabetical grouping:

```cpp
#include "tools/ocr/ocrlayout.h"
#include "tools/ocr/ocrpreprocess.h"
#include "tools/ocr/ocrscore.h"
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cmake --build build --config RelWithDebInfo --target ocr_tests`
Expected: FAIL to compile — `Cannot open include file: 'tools/ocr/ocrscore.h'`.

- [ ] **Step 4: Write the header**

Create `src/tools/ocr/ocrscore.h`:

```cpp
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/ocr/ocrengine.h"

/**
 * @brief How good a recognition result looks, in arbitrary points.
 *
 * Windows.Media.Ocr reports no per-word confidence, so the only signals
 * available are how much text came back and how plausibly it is shaped.
 * Character count dominates because the failure being guarded against is
 * "almost nothing came back"; the penalty on lone punctuation exists because
 * a badly scaled pass returns scattered marks, which would otherwise beat a
 * short clean result on line count alone.
 *
 * Only comparisons between scores are meaningful. A result that did not
 * succeed scores 0, so a failed pass can never displace a successful one.
 *
 * Pure function, kept separate so it can be tested without an engine.
 */
int ocrScoreResult(const OcrResult& result);
```

- [ ] **Step 5: Write the implementation**

Create `src/tools/ocr/ocrscore.cpp`:

```cpp
// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrscore.h"

namespace {

// Structure is worth something on its own: thirty short lines is a page,
// one long line of the same length is usually a misread
constexpr int LineBonus = 2;

// Enough to cancel a lone mark's own character, and its line's bonus over
// four such lines
constexpr int JunkPenalty = 2;

} // namespace

int ocrScoreResult(const OcrResult& result)
{
    if (result.status != OcrResult::Status::Ok) {
        return 0;
    }

    int score = 0;
    for (const OcrLine& line : result.lines) {
        score += LineBonus;

        if (line.words.isEmpty()) {
            score += line.text.trimmed().size();
            continue;
        }

        for (const OcrWord& word : line.words) {
            const QString text = word.text.trimmed();
            score += text.size();
            if (text.size() == 1 && !text.at(0).isLetterOrNumber()) {
                score -= JunkPenalty;
            }
        }
    }
    return score;
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cmake --build build --config RelWithDebInfo --target ocr_tests`
Then: `ctest --test-dir build -C RelWithDebInfo -R ocr_tests --output-on-failure`
Expected: PASS, all slots including the four new ones.

- [ ] **Step 7: Commit**

```bash
git add src/tools/ocr/ocrscore.h src/tools/ocr/ocrscore.cpp src/tools/CMakeLists.txt tests/ocr/CMakeLists.txt tests/ocr/tst_ocr.cpp
git commit -m "Add a score for comparing OCR passes against each other"
```

---

### Task 2: Tiling

This is the fix for the principal defect. `ocrScaleImage()` clamps the scale to `maxDimension / maxSide`, so a capture wider than the engine's ~2600 px limit can only ever be shrunk, never magnified — and those are precisely the captures whose glyphs are too small to read. Splitting the scaled image into overlapping tiles removes the ceiling: the engine still never sees an image over its limit, but the image as a whole can be as large as the glyphs need.

**Files:**
- Create: `src/tools/ocr/ocrtiling.h`
- Create: `src/tools/ocr/ocrtiling.cpp`
- Modify: `src/tools/CMakeLists.txt` (unguarded `target_sources` block)
- Modify: `tests/ocr/CMakeLists.txt`
- Test: `tests/ocr/tst_ocr.cpp`

**Interfaces:**
- Consumes: `OcrLine` from `tools/ocr/ocrengine.h`
- Produces:
  - `constexpr int OcrMaxTiles = 12;`
  - `QVector<QRect> ocrPlanTiles(const QSize& size, int maxDimension)`
  - `qreal ocrScaleWithinTileBudget(const QSize& size, qreal scale, int maxDimension)`
  - `QVector<OcrLine> ocrRemapLines(const QVector<OcrLine>& lines, const QPointF& origin)`
  - `QVector<OcrLine> ocrMergeTiledLines(const QVector<OcrLine>& lines)`

- [ ] **Step 1: Add the new files to both CMake source lists**

In `src/tools/CMakeLists.txt`, the unguarded OCR block's last two lines become:

```cmake
          ocr/ocrscore.h
          ocr/ocrscore.cpp
          ocr/ocrtiling.h
          ocr/ocrtiling.cpp)
```

In `tests/ocr/CMakeLists.txt`:

```cmake
add_executable(
  ocr_tests
  tst_ocr.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrpreprocess.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrlayout.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrscore.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrtiling.cpp)
```

- [ ] **Step 2: Write the failing tests**

Add `#include "tools/ocr/ocrtiling.h"` to the includes in `tst_ocr.cpp`.

Add this helper to the anonymous namespace, after `okResult()`:

```cpp
/// Whether every pixel of `size` falls inside at least one tile. Sampled on
/// a coarse grid plus the four corners: a gap large enough to lose a line of
/// text cannot hide between samples 13px apart.
bool tilesCoverEverything(const QSize& size, const QVector<QRect>& tiles)
{
    QVector<QPoint> samples{ QPoint(0, 0),
                             QPoint(size.width() - 1, 0),
                             QPoint(0, size.height() - 1),
                             QPoint(size.width() - 1, size.height() - 1) };
    for (int y = 0; y < size.height(); y += 13) {
        for (int x = 0; x < size.width(); x += 13) {
            samples.append(QPoint(x, y));
        }
    }
    for (const QPoint& point : samples) {
        bool covered = false;
        for (const QRect& tile : tiles) {
            if (tile.contains(point)) {
                covered = true;
                break;
            }
        }
        if (!covered) {
            return false;
        }
    }
    return true;
}
```

Then add a new section of test slots after the `ocrScoreResult` section:

```cpp
    // --- ocrPlanTiles ----------------------------------------------------

    void smallImagesAreASingleTile()
    {
        const QVector<QRect> tiles = ocrPlanTiles(QSize(800, 600), 2600);

        QCOMPARE(tiles.size(), 1);
        QCOMPARE(tiles.first(), QRect(0, 0, 800, 600));
    }

    void anUnlimitedEngineIsASingleTile()
    {
        const QVector<QRect> tiles = ocrPlanTiles(QSize(9000, 9000), 0);

        QCOMPARE(tiles.size(), 1);
        QCOMPARE(tiles.first(), QRect(0, 0, 9000, 9000));
    }

    void oversizedImagesAreSplitWithoutGaps()
    {
        const QSize size(5000, 3000);
        const QVector<QRect> tiles = ocrPlanTiles(size, 2000);

        QVERIFY(tiles.size() > 1);
        for (const QRect& tile : tiles) {
            QVERIFY(tile.width() <= 2000);
            QVERIFY(tile.height() <= 2000);
            QVERIFY(QRect(QPoint(0, 0), size).contains(tile));
        }
        QVERIFY(tilesCoverEverything(size, tiles));
    }

    void adjacentTilesOverlap()
    {
        // A line of text sitting on a seam has to be whole in one tile or
        // the merge has nothing to prefer
        const QVector<QRect> tiles = ocrPlanTiles(QSize(4000, 500), 2000);

        QVERIFY(tiles.size() >= 2);
        QVERIFY(tiles[0].right() > tiles[1].left());
    }

    // --- ocrScaleWithinTileBudget ----------------------------------------

    void aScaleThatAlreadyFitsIsUntouched()
    {
        QCOMPARE(ocrScaleWithinTileBudget(QSize(400, 300), 2.0, 2600), 2.0);
    }

    void anExtremeScaleIsReducedToTheTileBudget()
    {
        const QSize size(3840, 2160);
        const qreal scale = ocrScaleWithinTileBudget(size, 6.0, 2600);

        QVERIFY(scale < 6.0);
        const QSize scaled(qRound(size.width() * scale),
                           qRound(size.height() * scale));
        QVERIFY(ocrPlanTiles(scaled, 2600).size() <= OcrMaxTiles);
    }

    // --- ocrRemapLines ---------------------------------------------------

    void remappingTranslatesLineAndWordBoxes()
    {
        const QVector<OcrLine> remapped =
          ocrRemapLines({ makeWordLine({ QStringLiteral("abc") }, 5.0) },
                        QPointF(100, 200));

        QCOMPARE(remapped.first().boundingBox.left(), 100.0);
        QCOMPARE(remapped.first().boundingBox.top(), 205.0);
        QCOMPARE(remapped.first().words.first().boundingBox.left(), 100.0);
        QCOMPARE(remapped.first().words.first().boundingBox.top(), 205.0);
    }

    // --- ocrMergeTiledLines ----------------------------------------------

    void seamDuplicatesCollapseToTheLongerReading()
    {
        // The same line seen through two overlapping tiles: one tile saw all
        // of it, the other only the part inside its own bounds
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("full sentence here"),
                     QRectF(0, 0, 200, 20)),
            makeLine(QStringLiteral("full sen"), QRectF(0, 0, 190, 20)),
        };

        const QVector<OcrLine> merged = ocrMergeTiledLines(lines);

        QCOMPARE(merged.size(), 1);
        QCOMPARE(merged.first().text, QStringLiteral("full sentence here"));
    }

    void distinctNeighbouringLinesAreNotMerged()
    {
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("first"), QRectF(0, 0, 100, 20)),
            makeLine(QStringLiteral("second"), QRectF(0, 24, 100, 20)),
            makeLine(QStringLiteral("beside"), QRectF(120, 0, 100, 20)),
        };

        QCOMPARE(ocrMergeTiledLines(lines).size(), 3);
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cmake --build build --config RelWithDebInfo --target ocr_tests`
Expected: FAIL to compile — `Cannot open include file: 'tools/ocr/ocrtiling.h'`.

- [ ] **Step 4: Write the header**

Create `src/tools/ocr/ocrtiling.h`:

```cpp
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/ocr/ocrengine.h"

#include <QPointF>
#include <QRect>
#include <QSize>
#include <QVector>

// Most tiles one pass may be split into. Each tile is a separate engine
// round trip, so this is what stops an ambitious upscale turning into a
// wait the user notices.
constexpr int OcrMaxTiles = 12;

/**
 * @brief Split an image size into pieces the engine will accept.
 *
 * The engine refuses images over maxDimension, and the existing scaling code
 * handled that by clamping the scale — which means a capture wider than the
 * limit can only ever be shrunk, exactly when its glyphs are already too
 * small to read. Tiling removes that ceiling: the image as a whole can be as
 * large as the glyphs need, because the engine never sees it whole.
 *
 * Tiles overlap so that a line of text landing on a seam is complete in at
 * least one of them; ocrMergeTiledLines() then keeps the complete reading.
 * A size already within the limit, or an unlimited engine (maxDimension 0),
 * yields one tile covering everything, so callers need no special case.
 *
 * Pure function, kept separate so it can be tested without an engine.
 */
QVector<QRect> ocrPlanTiles(const QSize& size, int maxDimension);

/**
 * @brief The largest scale no greater than `scale` that stays within
 * OcrMaxTiles.
 *
 * Reducing the scale is better than truncating the image: the whole capture
 * is still recognized, just less magnified than would have been ideal.
 */
qreal ocrScaleWithinTileBudget(const QSize& size,
                               qreal scale,
                               int maxDimension);

/**
 * @brief Move tile-local boxes back into whole-image coordinates.
 *
 * The engine reports boxes relative to the image it was given, so every
 * tile's results arrive measured from that tile's own corner.
 */
QVector<OcrLine> ocrRemapLines(const QVector<OcrLine>& lines,
                               const QPointF& origin);

/**
 * @brief Collapse lines that overlapping tiles both reported.
 *
 * Two boxes covering mostly the same area are the same line seen twice. The
 * longer text wins, since the tile that contained more of the line read more
 * of it.
 */
QVector<OcrLine> ocrMergeTiledLines(const QVector<OcrLine>& lines);
```

- [ ] **Step 5: Write the implementation**

Create `src/tools/ocr/ocrtiling.cpp`:

```cpp
// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrtiling.h"

#include <QtGlobal>

namespace {

// Tiles advance by less than their width, leaving a 12% overlap. A line of
// text crossing a seam is then whole in one tile unless it is longer than
// the overlap itself, which at engine-limit sizes is hundreds of pixels.
constexpr qreal TileStep = 0.88;

// Boxes sharing more than this fraction of the smaller one's area are the
// same line reported twice, not two lines that happen to sit close together
constexpr qreal SameLineOverlap = 0.6;

// Guards the reduction loop; 0.8^24 is small enough that any image fits
constexpr int MaxBudgetAttempts = 24;

qreal area(const QRectF& rect)
{
    return rect.width() * rect.height();
}

} // namespace

QVector<QRect> ocrPlanTiles(const QSize& size, int maxDimension)
{
    QVector<QRect> tiles;
    if (size.isEmpty()) {
        return tiles;
    }

    if (maxDimension <= 0 ||
        (size.width() <= maxDimension && size.height() <= maxDimension)) {
        tiles.append(QRect(QPoint(0, 0), size));
        return tiles;
    }

    const int tileWidth = qMin(maxDimension, size.width());
    const int tileHeight = qMin(maxDimension, size.height());
    const int step = qMax(1, qRound(maxDimension * TileStep));

    for (int y = 0; y < size.height(); y += step) {
        // The last row is pulled back against the bottom edge rather than
        // left as a thin strip: a sliver a few pixels tall would cut every
        // glyph in it in half
        const int top = qMin(y, size.height() - tileHeight);
        for (int x = 0; x < size.width(); x += step) {
            const int left = qMin(x, size.width() - tileWidth);
            const QRect tile(left, top, tileWidth, tileHeight);
            if (!tiles.contains(tile)) {
                tiles.append(tile);
            }
            if (left + tileWidth >= size.width()) {
                break;
            }
        }
        if (top + tileHeight >= size.height()) {
            break;
        }
    }
    return tiles;
}

qreal ocrScaleWithinTileBudget(const QSize& size, qreal scale, int maxDimension)
{
    if (size.isEmpty() || maxDimension <= 0 || scale <= 0.0) {
        return scale;
    }

    for (int attempt = 0; attempt < MaxBudgetAttempts; ++attempt) {
        const QSize scaled(qMax(1, qRound(size.width() * scale)),
                           qMax(1, qRound(size.height() * scale)));
        if (ocrPlanTiles(scaled, maxDimension).size() <= OcrMaxTiles) {
            return scale;
        }
        scale *= 0.8;
    }
    return scale;
}

QVector<OcrLine> ocrRemapLines(const QVector<OcrLine>& lines,
                               const QPointF& origin)
{
    QVector<OcrLine> remapped;
    remapped.reserve(lines.size());
    for (OcrLine line : lines) {
        line.boundingBox.translate(origin);
        for (OcrWord& word : line.words) {
            word.boundingBox.translate(origin);
        }
        remapped.append(line);
    }
    return remapped;
}

QVector<OcrLine> ocrMergeTiledLines(const QVector<OcrLine>& lines)
{
    QVector<OcrLine> merged;
    for (const OcrLine& line : lines) {
        bool absorbed = false;
        for (OcrLine& kept : merged) {
            const QRectF shared = kept.boundingBox.intersected(line.boundingBox);
            const qreal smaller =
              qMin(area(kept.boundingBox), area(line.boundingBox));
            if (smaller <= 0.0 || area(shared) / smaller < SameLineOverlap) {
                continue;
            }
            if (line.text.size() > kept.text.size()) {
                kept = line;
            }
            absorbed = true;
            break;
        }
        if (!absorbed) {
            merged.append(line);
        }
    }
    return merged;
}
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cmake --build build --config RelWithDebInfo --target ocr_tests`
Then: `ctest --test-dir build -C RelWithDebInfo -R ocr_tests --output-on-failure`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/tools/ocr/ocrtiling.h src/tools/ocr/ocrtiling.cpp src/tools/CMakeLists.txt tests/ocr/CMakeLists.txt tests/ocr/tst_ocr.cpp
git commit -m "Add image tiling so large captures can be upscaled"
```

---

### Task 3: Better dark detection, and binarization

`hasDarkBackground()` averages the luminance of a 32×32 fast resample and inverts everything below 100. One number decides the fate of the whole capture, and a page with a large area of the opposite tone crosses the threshold the wrong way. Replacing it with two conditions that must both hold, and adding a thresholding step for antialiased text, gives the pipeline a third pass worth running.

**Files:**
- Modify: `src/tools/ocr/ocrpreprocess.h`
- Modify: `src/tools/ocr/ocrpreprocess.cpp:21-38` (replace `hasDarkBackground`), `:71` (its caller)
- Test: `tests/ocr/tst_ocr.cpp`

**Interfaces:**
- Consumes: nothing new
- Produces:
  - `bool ocrIsDarkBackground(const QImage& image)`
  - `QImage ocrBinarize(const QImage& image)`

- [ ] **Step 1: Write the failing tests**

Add this helper to the anonymous namespace in `tst_ocr.cpp`, after `tilesCoverEverything()`:

```cpp
/// A 100x100 image whose top `darkRows` rows are `dark` and the rest `light`
QImage twoTonePage(int darkRows, int dark, int light)
{
    QImage image(100, 100, QImage::Format_RGBA8888);
    image.fill(QColor(light, light, light));
    for (int y = 0; y < darkRows; ++y) {
        for (int x = 0; x < 100; ++x) {
            image.setPixelColor(x, y, QColor(dark, dark, dark));
        }
    }
    return image;
}
```

And a new section of test slots, after the `ocrScaleImage` section:

```cpp
    // --- ocrIsDarkBackground ---------------------------------------------

    void aDarkTerminalIsDark()
    {
        QVERIFY(ocrIsDarkBackground(twoTonePage(90, 18, 255)));
    }

    void aLightPageIsNotDark()
    {
        QVERIFY(!ocrIsDarkBackground(twoTonePage(10, 0, 255)));
    }

    void aMostlyDarkPageWithABrightImageIsDark()
    {
        // Mean luminance here is about 131, so a single-mean test calls this
        // light and leaves a dark page uninverted. The dominant tone and the
        // median both say otherwise.
        QVERIFY(ocrIsDarkBackground(twoTonePage(55, 30, 255)));
    }

    void anEvenlyMidTonedPageIsNotDark()
    {
        // Neither condition holds: the dominant tone is well above the
        // bottom third and the median is not low
        QVERIFY(!ocrIsDarkBackground(twoTonePage(50, 120, 200)));
    }

    // --- ocrBinarize -----------------------------------------------------

    void binarizingLeavesOnlyTwoTones()
    {
        // A ramp stands in for antialiased glyph edges
        QImage ramp(256, 8, QImage::Format_RGBA8888);
        for (int x = 0; x < 256; ++x) {
            for (int y = 0; y < 8; ++y) {
                ramp.setPixelColor(x, y, QColor(x, x, x));
            }
        }

        const QImage flat = ocrBinarize(ramp);

        QCOMPARE(flat.size(), ramp.size());
        for (int x = 0; x < 256; ++x) {
            const int value = flat.pixelColor(x, 0).red();
            QVERIFY(value == 0 || value == 255);
        }
        QCOMPARE(flat.pixelColor(0, 0), QColor(Qt::black));
        QCOMPARE(flat.pixelColor(255, 0), QColor(Qt::white));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cmake --build build --config RelWithDebInfo --target ocr_tests`
Expected: FAIL to compile — `'ocrIsDarkBackground': identifier not found`.

- [ ] **Step 3: Declare the two functions**

In `src/tools/ocr/ocrpreprocess.h`, add after the `ocrPadImage` declaration:

```cpp
/**
 * @brief Whether this capture is light text on a dark ground.
 *
 * The engine is trained mostly on dark-on-light text, so a terminal or a
 * dark theme recognizes markedly better inverted — and a page inverted
 * wrongly recognizes markedly worse, so the test has to be hard to fool. A
 * single mean is: one large region of the opposite tone drags it across the
 * threshold. This requires both that the most common tone is a dark one and
 * that the median pixel is dark, so a mixed-brightness capture is left alone
 * rather than inverted on the strength of its bright half.
 *
 * Exposed for testing; ocrNormalizeImage() is the intended entry point.
 */
bool ocrIsDarkBackground(const QImage& image);

/**
 * @brief Flatten a capture to pure black text on pure white.
 *
 * Subpixel antialiasing leaves glyph edges as a spread of intermediate
 * tones, which at small sizes is most of the glyph. Thresholding at the
 * split that best separates the image's two tonal populations (Otsu's
 * method) restores the hard edges the recognizer expects. It is not always
 * an improvement — on photographic backgrounds it destroys detail — so the
 * pipeline runs it as a candidate pass and keeps it only if it scores
 * better.
 */
QImage ocrBinarize(const QImage& image);
```

- [ ] **Step 4: Replace the detector and add the thresholder**

In `src/tools/ocr/ocrpreprocess.cpp`, delete the whole `hasDarkBackground()` function from the anonymous namespace (lines 21-38) and put these constants in its place:

```cpp
// Luminance histogram resolution for the dark-ground test. Coarse on
// purpose: it is looking for the dominant tonal region, not a mode.
constexpr int HistogramBuckets = 64;

// The dominant tone must fall in the bottom third of the range, and the
// median pixel must be below this, before the capture is inverted
constexpr int DarkMedianCeiling = 110;
```

Then add these two functions after `ocrPadImage()` in the same file:

```cpp
bool ocrIsDarkBackground(const QImage& image)
{
    if (image.isNull()) {
        return false;
    }

    // A resample is enough: this is a question about large regions, and the
    // smooth filter keeps a thin bright band from vanishing entirely
    const QImage sample =
      image.scaled(64, 64, Qt::IgnoreAspectRatio, Qt::SmoothTransformation)
        .convertToFormat(QImage::Format_Grayscale8);

    int buckets[HistogramBuckets] = { 0 };
    int total = 0;
    for (int y = 0; y < sample.height(); ++y) {
        const uchar* row = sample.constScanLine(y);
        for (int x = 0; x < sample.width(); ++x) {
            buckets[row[x] * HistogramBuckets / 256] += 1;
            ++total;
        }
    }
    if (total == 0) {
        return false;
    }

    int dominant = 0;
    int seen = 0;
    int median = 0;
    bool haveMedian = false;
    for (int i = 0; i < HistogramBuckets; ++i) {
        if (buckets[i] > buckets[dominant]) {
            dominant = i;
        }
        seen += buckets[i];
        if (!haveMedian && seen * 2 >= total) {
            median = i * 256 / HistogramBuckets;
            haveMedian = true;
        }
    }

    return dominant < HistogramBuckets / 3 && median < DarkMedianCeiling;
}

QImage ocrBinarize(const QImage& image)
{
    if (image.isNull()) {
        return image;
    }

    const QImage grey = image.convertToFormat(QImage::Format_Grayscale8);

    qint64 histogram[256] = { 0 };
    qint64 total = 0;
    for (int y = 0; y < grey.height(); ++y) {
        const uchar* row = grey.constScanLine(y);
        for (int x = 0; x < grey.width(); ++x) {
            histogram[row[x]] += 1;
            ++total;
        }
    }
    if (total == 0) {
        return image;
    }

    // Otsu's method: the threshold that maximizes the variance between the
    // two populations it separates
    qint64 sum = 0;
    for (int i = 0; i < 256; ++i) {
        sum += qint64(i) * histogram[i];
    }
    qint64 belowWeight = 0;
    qint64 belowSum = 0;
    qreal bestVariance = -1.0;
    int threshold = 127;
    for (int i = 0; i < 256; ++i) {
        belowWeight += histogram[i];
        if (belowWeight == 0) {
            continue;
        }
        const qint64 aboveWeight = total - belowWeight;
        if (aboveWeight == 0) {
            break;
        }
        belowSum += qint64(i) * histogram[i];
        const qreal belowMean = qreal(belowSum) / qreal(belowWeight);
        const qreal aboveMean = qreal(sum - belowSum) / qreal(aboveWeight);
        const qreal variance = qreal(belowWeight) * qreal(aboveWeight) *
                               (belowMean - aboveMean) * (belowMean - aboveMean);
        if (variance > bestVariance) {
            bestVariance = variance;
            threshold = i;
        }
    }

    QImage flat(grey.size(), QImage::Format_Grayscale8);
    for (int y = 0; y < grey.height(); ++y) {
        const uchar* source = grey.constScanLine(y);
        uchar* target = flat.scanLine(y);
        for (int x = 0; x < grey.width(); ++x) {
            target[x] = source[x] > threshold ? 255 : 0;
        }
    }
    return flat.convertToFormat(QImage::Format_RGBA8888);
}
```

- [ ] **Step 5: Point the existing caller at the new detector**

In `ocrNormalizeImage()`, `src/tools/ocr/ocrpreprocess.cpp:71`, change:

```cpp
    if (hasDarkBackground(prepared)) {
```

to:

```cpp
    if (ocrIsDarkBackground(prepared)) {
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cmake --build build --config RelWithDebInfo --target ocr_tests`
Then: `ctest --test-dir build -C RelWithDebInfo -R ocr_tests --output-on-failure`
Expected: PASS, including the pre-existing `ocrPadImage` and `ocrIdealScale` slots — `ocrNormalizeImage` is not directly covered, so a regression there would show up as those still passing; the dark-ground behaviour is covered by the new slots instead.

- [ ] **Step 7: Commit**

```bash
git add src/tools/ocr/ocrpreprocess.h src/tools/ocr/ocrpreprocess.cpp tests/ocr/tst_ocr.cpp
git commit -m "Harden dark-ground detection and add Otsu binarization"
```

---

### Task 4: The recognition pipeline

Everything so far is a part. This assembles them into the multi-pass strategy and is where the "second pass replaces a better first pass" defect is fixed. Recognition arrives as a callback rather than an `OcrEngine*`, which is what lets the whole strategy be tested with scripted results and no engine at all.

**Files:**
- Create: `src/tools/ocr/ocrpipeline.h`
- Create: `src/tools/ocr/ocrpipeline.cpp`
- Modify: `src/tools/ocr/ocrengine.h:41-48` (two new `OcrResult` fields)
- Modify: `src/tools/CMakeLists.txt` (unguarded `target_sources` block)
- Modify: `tests/ocr/CMakeLists.txt`
- Test: `tests/ocr/tst_ocr.cpp`

**Interfaces:**
- Consumes: `ocrScoreResult` (Task 1); `ocrPlanTiles`, `ocrScaleWithinTileBudget`, `ocrRemapLines`, `ocrMergeTiledLines`, `OcrMaxTiles` (Task 2); `ocrBinarize`, `ocrNormalizeImage`, `ocrScaleImage`, `ocrIdealScale` (Task 3 and existing)
- Produces:
  - `using OcrRecognizeFn = std::function<OcrResult(const QImage&, const QString&)>;`
  - `enum class OcrEffort { Normal, High };`
  - `OcrResult ocrRunPipeline(const QImage& capture, const OcrRecognizeFn& recognize, const QString& language, int maxDimension, OcrEffort effort = OcrEffort::Normal)`
  - `OcrResult::Source` enum and the `source` / `diagnostics` fields

- [ ] **Step 1: Add the new files to both CMake source lists**

`src/tools/CMakeLists.txt`, unguarded block, now ending:

```cmake
          ocr/ocrtiling.h
          ocr/ocrtiling.cpp
          ocr/ocrpipeline.h
          ocr/ocrpipeline.cpp)
```

`tests/ocr/CMakeLists.txt`:

```cmake
add_executable(
  ocr_tests
  tst_ocr.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrpreprocess.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrlayout.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrscore.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrtiling.cpp
  ${CMAKE_SOURCE_DIR}/src/tools/ocr/ocrpipeline.cpp)
```

- [ ] **Step 2: Write the failing tests**

Add `#include "tools/ocr/ocrpipeline.h"` to the includes in `tst_ocr.cpp`.

Add this fake engine to the anonymous namespace, after `twoTonePage()`:

```cpp
/**
 * Records every image it is handed and answers from a script, so the
 * pipeline's choices can be tested with no engine present. `reply` sees the
 * call index and the image it was given.
 */
struct FakeEngine
{
    QVector<QSize> seen;
    std::function<OcrResult(int index, const QImage& image)> reply;

    OcrRecognizeFn fn()
    {
        return [this](const QImage& image, const QString&) {
            const int index = seen.size();
            seen.append(image.size());
            return reply(index, image);
        };
    }
};

/// A result whose score is roughly `words` * 6
OcrResult scriptedResult(int words, qreal glyphHeight)
{
    QVector<OcrLine> lines;
    for (int i = 0; i < words; ++i) {
        OcrLine line;
        line.text = QStringLiteral("wordy");
        OcrWord word;
        word.text = line.text;
        word.boundingBox = QRectF(0, i * glyphHeight * 2, 50, glyphHeight);
        line.boundingBox = word.boundingBox;
        line.words.append(word);
        lines.append(line);
    }
    return okResult(lines);
}

QImage blankCapture(int width, int height)
{
    QImage image(width, height, QImage::Format_RGBA8888);
    image.fill(Qt::white);
    return image;
}
```

Then a new section of test slots at the end of the class, after the layout-preservation section:

```cpp
    // --- ocrRunPipeline --------------------------------------------------

    void aBetterLaterPassWins()
    {
        FakeEngine engine;
        // The first pass measures 10px glyphs, so the second runs at 4x and
        // reads far more
        engine.reply = [](int index, const QImage&) {
            return index == 0 ? scriptedResult(2, 10.0)
                              : scriptedResult(20, 40.0);
        };

        const OcrResult result =
          ocrRunPipeline(blankCapture(400, 300), engine.fn(), QString(), 2600);

        QCOMPARE(result.status, OcrResult::Status::Ok);
        QCOMPARE(result.lines.size(), 20);
    }

    void aWorseLaterPassIsDiscarded()
    {
        FakeEngine engine;
        engine.reply = [](int index, const QImage&) {
            return index == 0 ? scriptedResult(20, 10.0)
                              : scriptedResult(2, 40.0);
        };

        const OcrResult result =
          ocrRunPipeline(blankCapture(400, 300), engine.fn(), QString(), 2600);

        QCOMPARE(result.lines.size(), 20);
    }

    void anEmptyFirstPassTriggersTheUpscaleRetry()
    {
        FakeEngine engine;
        engine.reply = [](int index, const QImage&) {
            return index == 0 ? OcrResult{} : scriptedResult(5, 40.0);
        };

        const OcrResult result =
          ocrRunPipeline(blankCapture(400, 300), engine.fn(), QString(), 2600);

        QVERIFY(engine.seen.size() >= 2);
        const qreal ratio =
          qreal(engine.seen[1].width()) / qreal(engine.seen[0].width());
        QVERIFY2(qAbs(ratio - 4.0) < 0.05,
                 qPrintable(QStringLiteral("ratio was %1").arg(ratio)));
        QCOMPARE(result.lines.size(), 5);
    }

    void anOversizedPassIsTiledRatherThanShrunk()
    {
        FakeEngine engine;
        // Small glyphs, so the second pass wants a large upscale on an image
        // that is already over the limit
        engine.reply = [](int, const QImage&) {
            return scriptedResult(3, 8.0);
        };

        ocrRunPipeline(blankCapture(3000, 800), engine.fn(), QString(), 1000);

        QVERIFY(engine.seen.size() > 1);
        for (const QSize& size : engine.seen) {
            QVERIFY(size.width() <= 1000);
            QVERIFY(size.height() <= 1000);
        }
    }

    void highEffortRunsMorePassesThanNormal()
    {
        FakeEngine normal;
        normal.reply = [](int, const QImage&) {
            return scriptedResult(30, 40.0);
        };
        ocrRunPipeline(
          blankCapture(400, 300), normal.fn(), QString(), 2600);

        FakeEngine high;
        high.reply = [](int, const QImage&) {
            return scriptedResult(30, 40.0);
        };
        ocrRunPipeline(blankCapture(400, 300),
                       high.fn(),
                       QString(),
                       2600,
                       OcrEffort::High);

        QVERIFY(high.seen.size() > normal.seen.size());
    }

    void everyPassIsDescribedInTheDiagnostics()
    {
        FakeEngine engine;
        engine.reply = [](int, const QImage&) {
            return scriptedResult(30, 40.0);
        };

        const OcrResult result =
          ocrRunPipeline(blankCapture(400, 300), engine.fn(), QString(), 2600);

        QVERIFY(result.diagnostics.contains(QStringLiteral("400x300")));
        QVERIFY(result.diagnostics.contains(QStringLiteral("pass A")));
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cmake --build build --config RelWithDebInfo --target ocr_tests`
Expected: FAIL to compile — `Cannot open include file: 'tools/ocr/ocrpipeline.h'`.

- [ ] **Step 4: Extend `OcrResult`**

In `src/tools/ocr/ocrengine.h`, inside `struct OcrResult`, add the enum after the existing `Status` enum and the two fields after `errorMessage`:

```cpp
    // Where the text came from. Exact text is read back from the source
    // application and has no recognition error in it at all, which is worth
    // telling the user because it changes how much they should check it.
    enum class Source
    {
        Recognized,
        Exact,
    };

    QString fullText;
    QVector<OcrLine> lines;
    Status status{ Status::NoTextFound };
    // Engine-specific detail for EngineError (e.g. an HRESULT message).
    // User-facing wording for each status is built by the UI, so that
    // strings can be wrapped in tr() there.
    QString errorMessage;
    Source source{ Source::Recognized };
    // An account of what was attempted, for the user to read when a result
    // disappoints. Shown as-is; never parsed.
    QString diagnostics;
```

- [ ] **Step 5: Write the pipeline header**

Create `src/tools/ocr/ocrpipeline.h`:

```cpp
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/ocr/ocrengine.h"

#include <QImage>
#include <QString>

#include <functional>

/**
 * @brief Recognize one image. The pipeline's only contact with an engine.
 *
 * Taking recognition as a callback rather than an OcrEngine* is what keeps
 * the pipeline testable: the pass-selection logic is the part that goes
 * wrong, and this way it can be exercised with scripted results, with no
 * COM apartment, no WinRT and no installed language pack.
 */
using OcrRecognizeFn =
  std::function<OcrResult(const QImage& image, const QString& language)>;

enum class OcrEffort
{
    // Stop as soon as a result looks good enough
    Normal,
    // Run every pass and bias the scaling upward. What the user asks for
    // when the automatic answer disappointed them.
    High,
};

/**
 * @brief Recognize a capture, choosing between several attempts.
 *
 * A screenshot gives no clue how large its glyphs are until something has
 * read some, so the strategy cannot be decided up front. One pass measures,
 * a second re-runs at the scale that measurement implies, and a third
 * thresholds the image when the second still looks poor. Each is scored and
 * the best kept — an unscored second pass can be, and was, worse than the
 * first it replaced.
 *
 * Any pass whose scaled image exceeds `maxDimension` is tiled rather than
 * clamped, so the upscale a small-glyph capture needs is actually applied.
 * `maxDimension` of 0 means the engine has no limit.
 *
 * Lines come back in engine order; the caller applies ocrOrderLines().
 */
OcrResult ocrRunPipeline(const QImage& capture,
                         const OcrRecognizeFn& recognize,
                         const QString& language,
                         int maxDimension,
                         OcrEffort effort = OcrEffort::Normal);
```

- [ ] **Step 6: Write the pipeline implementation**

Create `src/tools/ocr/ocrpipeline.cpp`:

```cpp
// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrpipeline.h"

#include "tools/ocr/ocrpreprocess.h"
#include "tools/ocr/ocrscore.h"
#include "tools/ocr/ocrtiling.h"

#include <QStringList>

namespace {

// The first pass found nothing at all, which for a screenshot almost always
// means the glyphs are too small to resolve rather than that there is no
// text, so retry well upscaled before giving up
constexpr qreal EmptyResultRetryScale = 4.0;

// Roughly a handful of words. Below this the result is a failure worth
// another attempt; above it the extra engine round is not worth the wait.
constexpr int MinAcceptableScore = 40;

// High effort never magnifies less than this, whatever the first pass
// measured — the user asking for another try is evidence the measurement
// was not to be trusted
constexpr qreal HighEffortMinScale = 2.0;

struct Pass
{
    OcrResult result;
    int score = 0;
};

/// Rebuild fullText from the lines, so a caller that reads it directly sees
/// the same text the lines describe
void refreshFullText(OcrResult& result)
{
    QStringList texts;
    for (const OcrLine& line : result.lines) {
        texts.append(line.text);
    }
    result.fullText = texts.join(QLatin1Char('\n'));
}

/**
 * Recognize one already-scaled image, splitting it when the engine will not
 * take it whole and merging what the pieces found.
 */
OcrResult recognizeTiled(const QImage& image,
                         const OcrRecognizeFn& recognize,
                         const QString& language,
                         int maxDimension,
                         int& tileCount)
{
    const QVector<QRect> tiles = ocrPlanTiles(image.size(), maxDimension);
    tileCount = tiles.size();
    if (tiles.size() <= 1) {
        return recognize(image, language);
    }

    OcrResult merged;
    QString firstError;
    int failures = 0;

    for (const QRect& tile : tiles) {
        const OcrResult part = recognize(image.copy(tile), language);
        if (part.status == OcrResult::Status::NoLanguageInstalled ||
            part.status == OcrResult::Status::Unsupported) {
            // No other tile can do better; stop paying for them
            return part;
        }
        if (part.status == OcrResult::Status::EngineError) {
            ++failures;
            if (firstError.isEmpty()) {
                firstError = part.errorMessage;
            }
            continue;
        }
        merged.lines += ocrRemapLines(part.lines, tile.topLeft());
    }

    if (failures == tiles.size()) {
        merged.status = OcrResult::Status::EngineError;
        merged.errorMessage = firstError;
        return merged;
    }

    merged.lines = ocrMergeTiledLines(merged.lines);
    merged.status = merged.lines.isEmpty() ? OcrResult::Status::NoTextFound
                                           : OcrResult::Status::Ok;
    refreshFullText(merged);
    return merged;
}

QString describePass(const QString& name, qreal scale, int tiles, int score)
{
    return QStringLiteral("pass %1: scale %2, %3 tile(s), score %4")
      .arg(name)
      .arg(scale, 0, 'f', 2)
      .arg(tiles)
      .arg(score);
}

} // namespace

OcrResult ocrRunPipeline(const QImage& capture,
                         const OcrRecognizeFn& recognize,
                         const QString& language,
                         int maxDimension,
                         OcrEffort effort)
{
    QStringList notes;
    notes.append(QStringLiteral("capture %1x%2")
                   .arg(capture.width())
                   .arg(capture.height()));

    const QImage normalized = ocrNormalizeImage(capture);

    // Pass A exists to measure, not to win: nothing about a screenshot says
    // how tall its glyphs are until an engine has put boxes around some
    int tiles = 0;
    Pass best;
    best.result = recognizeTiled(ocrScaleImage(normalized, 1.0, maxDimension),
                                 recognize,
                                 language,
                                 maxDimension,
                                 tiles);
    best.score = ocrScoreResult(best.result);
    notes.append(describePass(QStringLiteral("A"), 1.0, tiles, best.score));

    if (best.result.status == OcrResult::Status::NoLanguageInstalled ||
        best.result.status == OcrResult::Status::Unsupported) {
        best.result.diagnostics = notes.join(QLatin1Char('\n'));
        return best.result;
    }

    const qreal measured = best.result.lines.isEmpty()
                             ? EmptyResultRetryScale
                             : ocrIdealScale(best.result.lines);
    qreal scale = effort == OcrEffort::High ? qMax(measured, HighEffortMinScale)
                                            : measured;
    // Tiling, not clamping, is what keeps the engine within its limit here —
    // clamping is the defect this pipeline exists to fix — but an unbounded
    // upscale would still turn into an unbounded number of round trips
    scale = ocrScaleWithinTileBudget(normalized.size(), scale, maxDimension);

    QImage scaled;
    if (qAbs(scale - 1.0) > 0.05) {
        // Scaling the untouched image rather than pass A's input avoids
        // compounding that pass's interpolation loss
        scaled = ocrScaleImage(normalized, scale, 0);
        Pass passB;
        passB.result = recognizeTiled(
          scaled, recognize, language, maxDimension, tiles);
        passB.score = ocrScoreResult(passB.result);
        notes.append(
          describePass(QStringLiteral("B"), scale, tiles, passB.score));
        if (passB.score > best.score) {
            best = passB;
        }
    }

    if (effort == OcrEffort::High || best.score < MinAcceptableScore) {
        if (scaled.isNull()) {
            scaled = ocrScaleImage(normalized, scale, 0);
        }
        Pass passC;
        passC.result = recognizeTiled(
          ocrBinarize(scaled), recognize, language, maxDimension, tiles);
        passC.score = ocrScoreResult(passC.result);
        notes.append(describePass(
          QStringLiteral("C (binarized)"), scale, tiles, passC.score));
        if (passC.score > best.score) {
            best = passC;
        }
    }

    notes.append(QStringLiteral("kept score %1").arg(best.score));
    refreshFullText(best.result);
    best.result.diagnostics = notes.join(QLatin1Char('\n'));
    return best.result;
}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cmake --build build --config RelWithDebInfo --target ocr_tests`
Then: `ctest --test-dir build -C RelWithDebInfo -R ocr_tests --output-on-failure`
Expected: PASS.

If `anEmptyFirstPassTriggersTheUpscaleRetry` fails on the 4.0 ratio, check that `ocrScaleImage` is being called with `0` and not `maxDimension` for pass B — that argument is the entire fix.

- [ ] **Step 8: Commit**

```bash
git add src/tools/ocr/ocrpipeline.h src/tools/ocr/ocrpipeline.cpp src/tools/ocr/ocrengine.h src/tools/CMakeLists.txt tests/ocr/CMakeLists.txt tests/ocr/tst_ocr.cpp
git commit -m "Add a scored multi-pass OCR pipeline with tiling"
```

---

### Task 5: Wire the worker to the exact reader and the pipeline

`readWindowText()` has been complete, compiled and linked since commit `cc1237f9` and nothing has ever called it. `OcrTool` already computes the screen rectangle it needs and `OcrResultsWindow` already stores it. This task connects the two ends and replaces the worker's inline two-pass logic with the pipeline.

**Files:**
- Modify: `src/tools/ocr/ocrresultswindow.h:24-40` (`OcrWorker`), `:80` (`startRecognition`)
- Modify: `src/tools/ocr/ocrresultswindow.cpp:27-48` (drop the local retry constant), `:50-95` (`OcrWorker`), `:274-289` (`startRecognition`)
- Test: no unit test. `readWindowText()` is cross-process COM against live windows and `OcrResultsWindow` is a widget; neither fits the pure-function test binary. Verification is that the app target compiles and that the pipeline's own tests still pass.

**Interfaces:**
- Consumes: `ocrRunPipeline`, `OcrEffort` (Task 4); `readWindowText` (existing, `tools/ocr/uiatextreader.h`)
- Produces: `OcrWorker(QImage image, QRect screenRect, QString language, quint64 runId, OcrEffort effort)`; `OcrResultsWindow::startRecognition(OcrEffort effort)`

- [ ] **Step 1: Widen `OcrWorker`**

In `src/tools/ocr/ocrresultswindow.h`, add the pipeline include next to the engine one:

```cpp
#include "tools/ocr/ocrengine.h"
#include "tools/ocr/ocrpipeline.h"
```

Replace the `OcrWorker` class body with:

```cpp
class OcrWorker : public QObject
{
    Q_OBJECT
public:
    OcrWorker(QImage image,
              QRect screenRect,
              QString language,
              quint64 runId,
              OcrEffort effort);

public slots:
    void process();

signals:
    void finished(const OcrResult& result, quint64 runId);

private:
    QImage m_image;
    // Where the capture sits in physical screen pixels; the exact-text
    // reader needs it to find the window the pixels came from
    QRect m_screenRect;
    QString m_language;
    quint64 m_runId;
    OcrEffort m_effort;
};
```

And change the private method declaration further down:

```cpp
    void startRecognition(OcrEffort effort = OcrEffort::Normal);
```

- [ ] **Step 2: Rewrite `OcrWorker` in the .cpp**

In `src/tools/ocr/ocrresultswindow.cpp`, add to the includes:

```cpp
#include "tools/ocr/ocrlayout.h"
#include "tools/ocr/ocrpipeline.h"
#include "tools/ocr/ocrpreprocess.h"
#include "tools/ocr/uiatextreader.h"
```

Delete the now-unused `EmptyResultRetryScale` constant and its comment from the anonymous namespace (lines 29-32) — the pipeline owns that constant now.

Replace the constructor and `process()` with:

```cpp
OcrWorker::OcrWorker(QImage image,
                     QRect screenRect,
                     QString language,
                     quint64 runId,
                     OcrEffort effort)
  : m_image(std::move(image))
  , m_screenRect(screenRect)
  , m_language(std::move(language))
  , m_runId(runId)
  , m_effort(effort)
{}

void OcrWorker::process()
{
#ifdef Q_OS_WIN
    // A screenshot is a lossy encoding of text the source application still
    // holds verbatim. Where it can be read back, recognition has nothing to
    // add. High effort means the user was unhappy with a recognized result,
    // and this path has already declined once, so it is not retried.
    if (m_effort == OcrEffort::Normal) {
        OcrResult exact = readWindowText(m_screenRect);
        if (exact.status == OcrResult::Status::Ok) {
            exact.source = OcrResult::Source::Exact;
            exact.diagnostics =
              QStringLiteral("read directly from the window under the "
                             "selection; no recognition was needed");
            exact.lines = ocrOrderLines(exact.lines);
            emit finished(exact, m_runId);
            return;
        }
    }
#endif

    // The engine is created inside the worker thread so nothing is shared
    // with the GUI thread
    QScopedPointer<OcrEngine> engine(OcrEngine::create());
    OcrEngine* raw = engine.data();

    OcrResult result = ocrRunPipeline(
      m_image,
      [raw](const QImage& image, const QString& language) {
          return raw->recognize(image, language);
      },
      m_language,
      engine->maxImageDimension(),
      m_effort);

    // Ordering is deterministic, so doing it here keeps it off the GUI
    // thread and lets the window re-join the lines for free
    result.lines = ocrOrderLines(result.lines);

    emit finished(result, m_runId);
}
```

- [ ] **Step 3: Pass the rectangle and the effort through `startRecognition`**

In `src/tools/ocr/ocrresultswindow.cpp`, change the signature and the worker construction:

```cpp
void OcrResultsWindow::startRecognition(OcrEffort effort)
{
    const quint64 runId = ++m_runId;
    m_statusLabel->hide();
    m_textEdit->setPlainText(QString());
    m_hasResult = false;
    m_busyDelay->start();

    // The worker communicates only through a queued connection, which Qt
    // severs safely if this window is closed mid-recognition; the thread
    // then finishes on its own and deletes itself
    auto* worker =
      new OcrWorker(m_capture.toImage(), m_screenRect, m_language, runId, effort);
    connect(
      worker, &OcrWorker::finished, this, &OcrResultsWindow::onWorkerFinished);
    startWorker(worker, &OcrWorker::finished);
}
```

The two existing callers — the constructor's trailing `startRecognition();` and `onLanguageChanged()` — need no change; they take the default.

- [ ] **Step 4: Build the application target**

Run: `cmake --build build --config RelWithDebInfo --target flameshot`
Expected: compiles and links cleanly. This is the only check that the Windows-only files agree with the new signatures, since the test binary does not compile them.

- [ ] **Step 5: Re-run the unit tests**

Run: `ctest --test-dir build -C RelWithDebInfo -R ocr_tests --output-on-failure`
Expected: PASS, unchanged.

- [ ] **Step 6: Commit**

```bash
git add src/tools/ocr/ocrresultswindow.h src/tools/ocr/ocrresultswindow.cpp
git commit -m "Read exact window text before recognizing, and run the pipeline"
```

---

### Task 6: Source badge, Try harder, and diagnostics

Recognition can now be retried, and it now knows whether it recognized anything or read it verbatim. Neither is worth anything until the window says so and gives the user the retry.

**Files:**
- Modify: `src/tools/ocr/ocrresultswindow.h:72-101` (slot and members)
- Modify: `src/tools/ocr/ocrresultswindow.cpp:121-145` (the hint label), `:199-205` (buttons), `:291-310` (`onWorkerFinished`)
- Test: no unit test; `OcrResultsWindow` is a widget outside the pure-function test binary. Verified by compiling the app target, and by the user's runtime checks.

**Interfaces:**
- Consumes: `OcrResult::source`, `OcrResult::diagnostics` (Task 4); `startRecognition(OcrEffort)` (Task 5)
- Produces: nothing other tasks depend on

**Deviation from the spec, deliberate:** the spec puts `diagnostics` on the status label's tooltip. The status label is hidden whenever the result is `Ok`, which is exactly when a curious user would look, so it goes on the always-visible hint/badge label instead, and on the status label as well when that is shown.

- [ ] **Step 1: Declare the button and the slot**

In `src/tools/ocr/ocrresultswindow.h`, add to the `private slots` block:

```cpp
    void retryHarder();
```

and to the members, beside `m_copyButton`:

```cpp
    QPushButton* m_tryHarderButton{ nullptr };
```

- [ ] **Step 2: Add the button**

In `src/tools/ocr/ocrresultswindow.cpp`, replace the button row:

```cpp
    auto* buttonLayout = new QHBoxLayout();
    buttonLayout->addStretch();
    m_tryHarderButton = new QPushButton(tr("Try harder"), this);
    m_tryHarderButton->setToolTip(
      tr("Recognize again with every strategy the tool has, including "
         "thresholding and a larger upscale. Slower."));
    // Nothing to improve on until a disappointing result exists
    m_tryHarderButton->setEnabled(false);
    connect(m_tryHarderButton,
            &QPushButton::clicked,
            this,
            &OcrResultsWindow::retryHarder);
    buttonLayout->addWidget(m_tryHarderButton);
    m_copyButton = new QPushButton(tr("Copy all"), this);
    connect(
      m_copyButton, &QPushButton::clicked, this, &OcrResultsWindow::copyAll);
    buttonLayout->addWidget(m_copyButton);
    layout->addLayout(buttonLayout);
```

- [ ] **Step 3: Implement the slot**

Add after `copyAll()`:

```cpp
void OcrResultsWindow::retryHarder()
{
    startRecognition(OcrEffort::High);
}
```

- [ ] **Step 4: Disable the button while a run is in flight**

In `startRecognition()`, after `m_hasResult = false;`:

```cpp
    m_tryHarderButton->setEnabled(false);
```

- [ ] **Step 5: Show the source and the diagnostics on the result**

Replace the body of `onWorkerFinished()` after the staleness check with:

```cpp
    m_busyDelay->stop();
    m_spinner->stop();
    m_spinner->hide();
    m_statusLabel->hide();

    m_result = result;
    m_hasResult = true;

    // Once there is a result, what the user needs to know is where it came
    // from — advice about framing the capture no longer applies to it
    const bool exact = result.source == OcrResult::Source::Exact;
    m_hintLabel->setText(exact
                           ? tr("Exact text, read from the window itself.")
                           : tr("Recognized from the captured image. Capture "
                                "close up and unblurred for the best result."));
    m_hintLabel->setToolTip(result.diagnostics);
    m_statusLabel->setToolTip(result.diagnostics);

    // Exact text has no recognition error in it, so there is nothing a
    // harder attempt could improve
    m_tryHarderButton->setEnabled(!exact);

    refreshText();
    if (result.status != OcrResult::Status::Ok) {
        showStatus(result);
    }
```

- [ ] **Step 6: Build the application target**

Run: `cmake --build build --config RelWithDebInfo --target flameshot`
Expected: compiles and links cleanly.

- [ ] **Step 7: Commit**

```bash
git add src/tools/ocr/ocrresultswindow.h src/tools/ocr/ocrresultswindow.cpp
git commit -m "Show the text's source and offer a harder retry"
```

---

### Task 7: Format, full verification, and handoff

**Files:**
- Modify: every file touched by Tasks 1-6 (formatting only)

**Interfaces:**
- Consumes: everything
- Produces: nothing

- [ ] **Step 1: Confirm the formatter actually runs**

```powershell
$env:PATH = "C:\Users\DTN2\AppData\Local\Programs\Git\mingw64\bin;$env:PATH"
& "C:\Users\DTN2\AppData\Local\Programs\Python\Python314\Lib\site-packages\clang_format\data\bin\clang-format.exe" --version
```

Expected: `clang-format version 11.1.0`. A silent exit or an empty line means the DLL path is wrong and every file will look formatted while nothing was touched — stop and fix it before continuing.

- [ ] **Step 2: Format the new and changed sources**

```powershell
$env:PATH = "C:\Users\DTN2\AppData\Local\Programs\Git\mingw64\bin;$env:PATH"
$fmt = "C:\Users\DTN2\AppData\Local\Programs\Python\Python314\Lib\site-packages\clang_format\data\bin\clang-format.exe"
& $fmt -i --style=file `
  src/tools/ocr/ocrscore.h src/tools/ocr/ocrscore.cpp `
  src/tools/ocr/ocrtiling.h src/tools/ocr/ocrtiling.cpp `
  src/tools/ocr/ocrpipeline.h src/tools/ocr/ocrpipeline.cpp `
  src/tools/ocr/ocrpreprocess.h src/tools/ocr/ocrpreprocess.cpp `
  src/tools/ocr/ocrengine.h `
  src/tools/ocr/ocrresultswindow.h src/tools/ocr/ocrresultswindow.cpp `
  tests/ocr/tst_ocr.cpp
```

- [ ] **Step 3: Rebuild and retest after formatting**

```bash
cmake --build build --config RelWithDebInfo --target ocr_tests
ctest --test-dir build -C RelWithDebInfo -R ocr_tests --output-on-failure
cmake --build build --config RelWithDebInfo --target flameshot
```

Expected: tests PASS, app target links.

- [ ] **Step 4: Build the installer configuration once**

The `USE_PORTABLE_CONFIG=OFF` branches are invisible to a default local build, and a missing include there builds clean here and fails the release.

```bash
cmake -S . -B build-installer -G "Visual Studio 18 2026" -A x64 "-DCMAKE_PREFIX_PATH=C:\Qt\6.9.3\msvc2022_64" -DUSE_PORTABLE_CONFIG=OFF
cmake --build build-installer --config Release --target flameshot
```

Expected: compiles and links. This work adds no config keys, so it should be unaffected — the check is cheap insurance, not an expected failure.

- [ ] **Step 5: Commit the formatting**

```bash
git add -u
git commit -m "Apply clang-format to the new OCR sources"
```

- [ ] **Step 6: Hand runtime verification to the user**

Do not run `phramer.exe`. Report to the user that the branch is ready and list what only they can check:

1. Select text in Windows Terminal, VS Code and a browser — the badge should read "Exact text, read from the window itself" and the text should be verbatim, with **Try harder** disabled.
2. Capture a whole screen of small text — previously returned nothing; should now return text, and the badge tooltip should show more than one tile.
3. Capture a dark-themed window, and a light page containing a dark code block — neither should come back as garbage.
4. Capture something deliberately marginal (small, blurry) and press **Try harder** — the result should change, and the tooltip should list pass C.
5. Capture a photograph with no text — should still say "No text was found in the selection", with diagnostics in the tooltip.

---

## Self-Review

**Spec coverage:**

| Spec section | Task |
|---|---|
| Exact-text path wired in | 5 |
| Tiling replaces the scale clamp | 2 (mechanism), 4 (applied) |
| Passes scored, best kept | 1 (score), 4 (selection) |
| Histogram dark detection | 3 |
| Otsu binarization, pass C | 3 (function), 4 (pass) |
| `OcrResult::source` / `diagnostics` | 4 |
| Source badge, Try harder, tooltip | 6 |
| Error-handling table | 4 (`recognizeTiled`), 5 (UIA fallthrough) |
| Test list | 1, 2, 3, 4 |
| No new config keys | all — none added |

**Deviations from the spec, both deliberate and noted at their task:**
- Diagnostics go on the hint/badge label's tooltip as well as the status label's, because the status label is hidden on success (Task 6).
- `ocrOrderLines()` stays in `OcrWorker` rather than moving into the pipeline, so the exact and recognized paths share one ordering call (Task 5).
