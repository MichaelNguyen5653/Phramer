// SPDX-License-Identifier: GPL-3.0-or-later

#include "windowsnap.h"

#include <QtMath>

namespace WindowSnap {

QRect windowAt(const QVector<QRect>& windows, const QPoint& point)
{
    for (const QRect& window : windows) {
        if (window.contains(point)) {
            return window;
        }
    }
    return {};
}

QRect toOverlay(const QRect& physical,
                const QPoint& overlayNativeOrigin,
                qreal devicePixelRatio,
                const QRect& overlay)
{
    const qreal ratio = devicePixelRatio > 0 ? devicePixelRatio : 1.0;
    // Edges are converted rather than the corner and size, so rounding can
    // never leave a one-pixel gap between the frame and the selection
    const auto toLogical = [&](int value, int origin) {
        return qRound((value - origin) / ratio);
    };
    const int left = toLogical(physical.left(), overlayNativeOrigin.x());
    const int top = toLogical(physical.top(), overlayNativeOrigin.y());
    const int right =
      toLogical(physical.left() + physical.width(), overlayNativeOrigin.x());
    const int bottom =
      toLogical(physical.top() + physical.height(), overlayNativeOrigin.y());

    const QRect mapped =
      QRect(QPoint(left, top), QSize(right - left, bottom - top))
        .intersected(overlay);
    return mapped.isEmpty() ? QRect() : mapped;
}

} // namespace WindowSnap
