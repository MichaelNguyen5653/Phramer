// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/ocr/ocrengine.h"

#include <QString>
#include <QVector>

/**
 * @brief Put recognized lines into the order a person would read them.
 *
 * The engine emits lines in its own order, which does not always follow the
 * page: side-by-side windows and two-column text come back interleaved, and
 * stray fragments land wherever they were found. This reconstructs the
 * layout geometrically — column bands first, then visual rows within each
 * band, then left to right within a row.
 *
 * Column splitting is deliberately conservative: it needs a wide, full-height
 * gutter with real content on both sides, so ordinary prose is left alone.
 *
 * Pure function, kept separate so it can be tested without an engine.
 */
QVector<OcrLine> ocrOrderLines(const QVector<OcrLine>& lines);

enum class OcrTextLayout
{
    // Rebuild the page: one output line per visual row, leading indentation
    // and the gaps between fragments turned back into spaces. This is what
    // makes captured code and terminal output paste back usably.
    Preserve,
    // One trimmed line per visual row, single-spaced. Useful when the
    // capture's own alignment is noise.
    Plain,
    // Preserve, then merge lines that continue the same wrapped paragraph,
    // so prose is not hard-wrapped at whatever width the screenshot was.
    // Wrong for terminals and code, where short lines really are separate.
    Join,
};

/**
 * @brief Join ordered lines into the text shown to the user.
 *
 * Recognizers report fragments, not lines: a run of whitespace inside one
 * visual row usually arrives as two separate boxes, and neither the leading
 * indentation nor the width of the gap survives in the text. Both are still
 * present in the geometry, so this reconstructs them by measuring a typical
 * character and converting distances back into spaces.
 */
QString ocrAssembleText(const QVector<OcrLine>& lines, OcrTextLayout layout);

/**
 * @brief Width of a typical character across these lines, or 0 if unknown.
 *
 * Exposed for testing; it is the scale that converts a pixel gap into a
 * number of spaces.
 */
qreal ocrEstimateCharWidth(const QVector<OcrLine>& lines);

/**
 * @brief Whether `next` continues the paragraph that `current` starts.
 *
 * Exposed for testing; ocrAssembleText() is the intended entry point.
 */
bool ocrIsWrappedContinuation(const QRectF& current, const QRectF& next);
