// SPDX-License-Identifier: GPL-3.0-or-later

#include "flowpacking.h"

#include <algorithm>

namespace FlowPacking {

Result pack(const QVector<QSize>& sizes,
            int width,
            int horizontalSpacing,
            int verticalSpacing)
{
    Result result;
    result.rects.reserve(sizes.size());

    int x = 0;
    int y = 0;
    int rowHeight = 0;
    int rowStart = 0;

    // Centring needs the row's final height, so each row is settled when
    // the next one starts, and once more at the end
    const auto finishRow = [&](int end) {
        for (int i = rowStart; i < end; ++i) {
            QRect& r = result.rects[i];
            r.moveTop(y + (rowHeight - r.height()) / 2);
        }
    };

    for (int i = 0; i < sizes.size(); ++i) {
        const QSize size = sizes.at(i);
        const bool rowHasItems = i > rowStart;
        if (rowHasItems && x + size.width() > width) {
            finishRow(i);
            y += rowHeight + verticalSpacing;
            x = 0;
            rowHeight = 0;
            rowStart = i;
        }
        result.rects.append(QRect(QPoint(x, y), size));
        x += size.width() + horizontalSpacing;
        rowHeight = std::max(rowHeight, size.height());
    }
    if (!sizes.isEmpty()) {
        finishRow(static_cast<int>(sizes.size()));
        result.height = y + rowHeight;
    }
    return result;
}

} // namespace FlowPacking
