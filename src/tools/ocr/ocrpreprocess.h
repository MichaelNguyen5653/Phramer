// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/ocr/ocrengine.h"

#include <QImage>

/**
 * @brief Pad an image out to a workable minimum size.
 *
 * The engine returns nothing at all for very small crops, so a tight
 * selection around a couple of words has to be given room. The border is
 * filled with the top-left pixel so it reads as more background rather than
 * as an edge the recognizer has to explain.
 *
 * Images already at least minSide on both axes are returned unchanged.
 */
QImage ocrPadImage(const QImage& image, int minSide = 64, int border = 8);

/**
 * @brief Put a screenshot crop into the form the engine expects, unscaled.
 *
 * Converts to QImage::Format_RGBA8888, inverts light-on-dark captures
 * (terminals, dark themes — the engine is markedly less accurate on them),
 * and pads. Scaling is deliberately not done here: the right factor is not
 * knowable until a first recognition pass has measured the glyphs.
 *
 * Pure function, kept separate so it can be tested without an engine.
 */
QImage ocrNormalizeImage(const QImage& image);

/**
 * @brief Scale a normalized image, keeping it within the engine's limit.
 *
 * `scale` is clamped so neither side exceeds maxDimension (0 means
 * unlimited), and an image already over the limit is scaled down to fit
 * regardless of `scale`. Always returns Format_RGBA8888.
 */
QImage ocrScaleImage(const QImage& image, qreal scale, int maxDimension = 0);

/**
 * @brief The scale that would put recognized glyphs at the engine's sweet
 * spot.
 *
 * Selection size is a bad proxy for glyph size (a full-window capture of a
 * terminal is large but its text is tiny), so this measures the boxes an
 * actual pass produced. Word boxes are preferred over line boxes because a
 * line box spans the whole line and is skewed by any oversized glyph in it.
 *
 * Returns 1.0 when there is nothing to measure, so callers can treat "no
 * change" and "no data" alike.
 */
qreal ocrIdealScale(const QVector<OcrLine>& lines);
