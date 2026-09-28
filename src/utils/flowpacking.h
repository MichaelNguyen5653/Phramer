// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QRect>
#include <QSize>
#include <QVector>

/**
 * @brief Packs items left to right into rows, starting a new row whenever
 * the next item would cross `width`.
 *
 * The geometry behind FlowLayout, kept free of widgets so it can be tested
 * without a QApplication. An item wider than `width` still gets a row of its
 * own rather than being dropped. Items in a row share its top edge and are
 * centred vertically on its tallest item.
 */
namespace FlowPacking {

struct Result
{
    // One per input size, in input order, relative to (0,0)
    QVector<QRect> rects;
    // Total height of every row plus the spacing between them; 0 when empty
    int height = 0;
};

Result pack(const QVector<QSize>& sizes,
            int width,
            int horizontalSpacing,
            int verticalSpacing);

} // namespace FlowPacking
