// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QImage>
#include <QPixmap>
#include <QRect>
#include <QVector>

/**
 * @brief The pixels bordering a rectangle, one line per side.
 *
 * This is everything the frosted fill is allowed to know about the image.
 * A side is empty when the rectangle touches the image edge there, because
 * the only row left on that side would be the rectangle's own content.
 */
struct FrostedEdges
{
    QVector<QRgb> top;
    QVector<QRgb> bottom;
    QVector<QRgb> left;
    QVector<QRgb> right;

    bool operator==(const FrostedEdges& other) const = default;
};

// rect is in the pixmap's own pixels, not its device-independent size.
// Never reads a pixel inside rect.
FrostedEdges frostedEdges(const QPixmap& source, const QRect& rect);

/**
 * @brief Redaction fill that looks like frosted glass.
 *
 * Pixelating or blurring the covered pixels themselves is reversible
 * (https://github.com/bishopfox/unredacter), so the fill is built from edges
 * alone: each side is smoothed until only its broad colour is left, the four
 * are blended across the rectangle as a Coons patch so the fill meets its
 * surroundings without a seam, and a fixed grain keeps it from reading as a
 * flat gradient.
 *
 * strength is the tool size; larger means softer.
 */
QImage frostedFill(const FrostedEdges& edges, const QSize& size, int strength);
