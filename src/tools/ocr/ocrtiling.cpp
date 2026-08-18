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
            const QRectF shared =
              kept.boundingBox.intersected(line.boundingBox);
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
