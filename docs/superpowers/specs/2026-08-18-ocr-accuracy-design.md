# OCR accuracy and consistency

Status: approved design, not yet implemented
Date: 2026-08-18

## Problem

The OCR tool works but is inconsistent. Some captures yield no text at all.
Reading the pipeline found five causes, three of them defects rather than
missing polish.

### 1. The exact-text path exists and is never called

`readWindowText()` in `src/tools/ocr/uiatextreader.cpp` reads text out of the
window under a screen rectangle through UI Automation. It is complete,
compiled, and linked (`uiautomationcore`, `dwmapi`), and nothing calls it.
`OcrTool::pressed()` computes `m_captureScreenRect`, passes it to
`OcrResultsWindow`, which stores it in `m_screenRect` and never reads it.
`OcrWorker` is not given the rectangle at all.

For terminals, browsers, editors and native controls this path returns the
source text verbatim. Every recognition failure mode — small glyphs,
antialiasing, ambiguous glyph pairs — does not apply to it.

### 2. Large captures can never be upscaled

`ocrScaleImage()` clamps the requested scale to `maxDimension / maxSide`.
`WindowsOcrEngine::maxImageDimension()` reports roughly 2600. A selection
3000 px wide therefore gets a clamp of about 0.86, so the glyph-sized second
pass in `OcrWorker::process()` is forced *downward* no matter how small the
measured glyphs were. Large captures on high-DPI displays — a full window, a
whole screen — are exactly the case where glyphs are smallest, and they are
the case that can never be helped. This is the principal cause of "no text at
all".

### 3. The second pass can lose to itself

`OcrWorker::process()` accepts the second pass whenever its status is `Ok`,
without comparing it to the first. A pass that finds three lines replaces a
pass that found thirty.

### 4. Dark-background detection is one global mean

`hasDarkBackground()` averages the luminance of a 32x32 fast resample and
inverts the whole image below 100. A light page containing a dark code block,
or a dark page containing a bright image, crosses that threshold the wrong way
and the inversion then damages the entire capture rather than helping part of
it.

### 5. One attempt, no recourse, no diagnostics

There is a single automatic strategy. When it fails the user is told "No text
was found in the selection" and has nothing to try, and nothing reports what
was attempted.

## Design

### Two sources, tried in order

`OcrWorker::process()` becomes a two-stage chain:

```
1. Exact       readWindowText(screenRect)          Windows only
                 -> status Ok: done, source = Exact
2. Recognized  ocrRunPipeline(image, recognizeFn, language, ...)
                 -> normalize, candidate passes, tiling, scoring
```

A UIA miss is the normal outcome for images, games and remote sessions.
`readWindowText()` already reports it as `NoTextFound`, so stage 1 falling
through is ordinary control flow, not error handling.

`readWindowText()` stays in the worker and never moves into the pipeline. It
is COM, Windows-only, and cross-process; the pipeline has to stay linkable
into the test binary without any of that.

### New files

All under `src/tools/ocr/`.

| File | Responsibility |
|---|---|
| `ocrpipeline.{h,cpp}` | Orchestrates the candidate passes and returns the winner |
| `ocrtiling.{h,cpp}` | Tile geometry, box remapping, seam dedup. Pure |
| `ocrscore.{h,cpp}` | Scores an `OcrResult`. Pure |

The pipeline takes recognition as a callback rather than an `OcrEngine*`:

```cpp
using OcrRecognizeFn =
  std::function<OcrResult(const QImage&, const QString& language)>;

enum class OcrEffort { Normal, High };

OcrResult ocrRunPipeline(const QImage& capture,
                         const OcrRecognizeFn& recognize,
                         const QString& language,
                         int maxDimension,
                         OcrEffort effort = OcrEffort::Normal);
```

That is the whole testability story: the pass-selection logic is exercised
with a fake `recognize` returning scripted results, with no engine, no COM and
no Windows dependency.

`ocrpreprocess` gains two exported functions:

- `bool ocrIsDarkBackground(const QImage&)` — replaces the private
  `hasDarkBackground()`. Builds a 64-bucket luminance histogram of a resample
  and reports dark only when the dominant mode sits in the lower third *and*
  the median luminance is below 110. Both conditions must hold, so a
  mixed-brightness capture is left alone rather than inverted wrongly.
- `QImage ocrBinarize(const QImage&)` — grayscale, global Otsu threshold,
  returns `Format_RGBA8888` black on white.

### Candidate passes

Sequential, early-exit, at most three engine rounds.

**Pass A** — the normalized image at native scale, downscaled only if it
exceeds `maxDimension`. Not a candidate for its own sake: its word boxes are
what make glyph height measurable.

**Pass B** — the normalized image at `ocrIdealScale(A.lines)`, tiled when the
scaled size exceeds `maxDimension`. When pass A produced no lines the scale is
`EmptyResultRetryScale` (4.0) as today. Tiling is what removes the clamp: the
upscale is no longer limited by what the engine accepts in one image, it is
satisfied by handing the engine several images.

**Pass C** — pass B's geometry with `ocrBinarize()` applied. Runs only when the
best score so far is below `MinAcceptableScore` (40, roughly a handful of
words — below that the result is a failure worth another attempt, above it the
extra engine round is not worth the latency), or unconditionally under
`OcrEffort::High`.

The winner is the highest-scoring result; ties go to the earlier pass, so an
equal-scoring later pass never displaces an earlier one. Under
`OcrEffort::High` the early exit is skipped, all three passes run, and pass B
uses `max(idealScale, 2.0)`.

### Scoring

`ocrScoreResult(const OcrResult&)` returns a single `int`:

```
score = sum over words of:
            + length(word.text)              recognized characters
            - 2  if length(word.text) == 1 and it is not alphanumeric
        + 2 * number of lines                rewards structure over a blob
```

Rationale: Windows.Media.Ocr exposes no confidence value, so the only signals
available are how much text came back and how plausibly it is shaped. Total
character count is the dominant term because the failure being fixed is
"almost nothing came back". The single-character penalty exists because a
badly scaled pass returns scattered punctuation fragments, which would
otherwise let noise outscore a clean result on line count alone.

### Tiling

`ocrPlanTiles(const QSize& size, int maxDimension)` returns tile rectangles:

- Tile side is `maxDimension`, stepped by `maxDimension * 0.88`, giving 12%
  overlap. A line of text landing on a seam is then whole in at least one
  tile, provided it is shorter than the overlap; longer lines are handled by
  the dedup below merging the two halves' boxes and keeping the longer text.
- The tile count is capped at `MaxTiles` (12). When a plan would exceed it,
  `ocrRunPipeline` lowers the pass's scale (repeatedly multiplying by 0.8) and
  re-plans until the count fits, so the cap bounds the work done rather than
  truncating the image.
- A size already within `maxDimension` yields one tile equal to the whole
  image, so the tiled and untiled paths are the same code.

Each tile is recognized independently. `ocrRemapLines(lines, tileOrigin)`
translates the boxes back into scaled-image space. `ocrMergeTiledLines()` then
deduplicates: two lines whose boxes overlap by more than 60% of the smaller
box's area are the same line, and the longer text wins. Surviving lines go
through the existing `ocrOrderLines()`.

One tile failing with `EngineError` does not abort the pass; its lines are
simply absent. A pass whose every tile failed reports `EngineError` carrying
the first tile's message.

### Result changes

`OcrResult` gains two fields:

```cpp
enum class Source { Recognized, Exact };
Source source{ Source::Recognized };
// Human-readable account of what was attempted; shown as a tooltip, never
// parsed
QString diagnostics;
```

The struct is internal to the fork, so extending it is safe. `diagnostics`
records capture size, chosen scale, tile count, which passes ran, each pass's
score, and the winner.

### UI

`OcrResultsWindow` changes:

- The standing hint label is replaced, once a result arrives, by a source
  badge: "Exact text from the window" or "Recognized from the image". The
  static advice about capturing close up stays until then, since before a
  result it is still the only useful thing to say.
- A **Try harder** button beside **Copy all**, enabled once a result exists,
  re-runs the pipeline with `OcrEffort::High`. It is disabled while a run is in
  flight, and while the source is `Exact` — exact text cannot be improved.
- `diagnostics` becomes the status label's tooltip.

No new configuration keys. The pipeline is automatic; effort is a per-run user
action, not a setting.

`OcrWorker`'s constructor takes the screen rectangle and the effort:

```cpp
OcrWorker(QImage image, QRect screenRect, QString language,
          quint64 runId, OcrEffort effort);
```

The existing `runId` staleness check already covers a second run started
before the first returns, so **Try harder** needs no new concurrency
handling.

### Error handling summary

| Condition | Behaviour |
|---|---|
| UIA has no provider, or times out | Fall through to recognition |
| UIA returns text outside the selection | Already rejected inside `readFromWindow()`; falls through |
| One tile errors | Pass continues without that tile's lines |
| Every tile errors | Pass reports `EngineError`, first message kept |
| All passes empty | `NoTextFound`, diagnostics explain what was tried |
| No OCR language installed | Unchanged; existing status and guidance |

## Testing

`tests/ocr/tst_ocr.cpp` is an existing `QTest` binary over the pure OCR
helpers, built with `-DBUILD_OCR_TESTS=ON` and run through ctest. It gains:

- `ocrPlanTiles`: a size under the limit yields one full-image tile; an
  oversized size yields overlapping tiles covering every pixel; the plan never
  exceeds `MaxTiles`.
- `ocrRemapLines`: boxes translate by the tile origin.
- `ocrMergeTiledLines`: duplicated seam lines collapse to one, the longer text
  survives, and distinct lines that merely sit near each other do not merge.
- `ocrScoreResult`: more characters outscores fewer; a scatter of single
  punctuation marks loses to a shorter clean result; equal scores preserve
  order.
- `ocrIsDarkBackground`: a dark terminal is dark, a light page is not, and a
  light page with a dark code block is not.
- `ocrBinarize`: an antialiased grey ramp comes back two-valued.
- `ocrRunPipeline` with a fake `recognize`: pass B replaces a weaker pass A; a
  stronger pass A survives a weaker pass B; an empty pass A triggers the 4.0
  retry; an oversized image is tiled rather than downscaled; `High` effort runs
  all three passes.

`tests/ocr/CMakeLists.txt` gains `ocrpipeline.cpp`, `ocrtiling.cpp` and
`ocrscore.cpp`, in the same compiled-straight-into-the-test-binary style it
already uses for `ocrpreprocess.cpp` and `ocrlayout.cpp`.

Manual verification is required for what unit tests cannot reach, on the local
build:

1. Exact text from a terminal, VS Code and a browser.
2. A screen-sized capture of small text, which previously returned nothing.
3. A dark-themed capture, and a light capture containing a dark code block.
4. **Try harder** improving a deliberately marginal capture.
5. An image with no text, confirming `NoTextFound` with diagnostics.

## Out of scope

- Bundling a second engine (Tesseract, or an ONNX model such as PaddleOCR or
  RapidOCR). It raises the accuracy ceiling on hard captures but adds a
  dependency, model files, and MSI and CPack work. The `OcrEngine` interface
  already accommodates it when that is wanted.
- Per-region inversion, and deskewing. Screenshots are axis-aligned and
  unrotated, so deskewing has nothing to correct.
- Non-Windows exact-text providers. `NullOcrEngine` and the `Q_OS_WIN` guards
  keep other platforms compiling, which is all they need.
