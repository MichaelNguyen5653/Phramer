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
 * handled that by clamping the scale -- which means a capture wider than the
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
