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
 * the best kept -- an unscored second pass can be, and was, worse than the
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
