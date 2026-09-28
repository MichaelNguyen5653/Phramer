// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/canvasgeometry.h"

#include <cmath>

namespace CanvasGeometry {

qreal zoomStep(qreal current, int notches)
{
    if (notches == 0) {
        return current;
    }
    qreal next = current * std::pow(ZoomFactor, notches);

    // Landing exactly on actual size whenever a step would cross it is what
    // lets the wheel alone get back to 100%. Without it the user can only
    // approach 1.0 and never hit it, because the steps are geometric.
    if ((current < 1.0 && next > 1.0) || (current > 1.0 && next < 1.0)) {
        next = 1.0;
    }
    return qBound(MinZoom, next, MaxZoom);
}

int workspaceMargin(const QSize& imageSize, qreal devicePixelRatio)
{
    const int larger = qMax(imageSize.width(), imageSize.height());
    const int wanted =
      qBound(MinWorkspaceMargin, qRound(larger * 0.4), MaxWorkspaceMargin);
    const auto whole = [devicePixelRatio](int margin) {
        const qreal physical = margin * devicePixelRatio;
        return std::abs(physical - std::round(physical)) < 1e-6;
    };
    // The nearest whole-pixel margin at or above the wanted one, then below
    // it, without leaving the bounds. Every common scale (1.25, 1.5, 1.75,
    // 2.25...) finds one within a few steps. A ratio with none in range keeps
    // the wanted margin and accepts a resampled image.
    for (int margin = wanted; margin <= MaxWorkspaceMargin; ++margin) {
        if (whole(margin)) {
            return margin;
        }
    }
    for (int margin = wanted - 1; margin >= MinWorkspaceMargin; --margin) {
        if (whole(margin)) {
            return margin;
        }
    }
    return wanted;
}

QRect toPhysical(const QRect& logical, qreal devicePixelRatio)
{
    if (logical.isEmpty()) {
        return {};
    }
    const int left =
      static_cast<int>(std::floor(logical.left() * devicePixelRatio));
    const int top =
      static_cast<int>(std::floor(logical.top() * devicePixelRatio));
    const int right = static_cast<int>(
      std::ceil((logical.left() + logical.width()) * devicePixelRatio));
    const int bottom = static_cast<int>(
      std::ceil((logical.top() + logical.height()) * devicePixelRatio));
    return QRect(left, top, right - left, bottom - top);
}

QRect exportRect(const QRect& image,
                 const QVector<QRect>& annotations,
                 const QRect& workspace)
{
    QRect area = image;
    for (const QRect& annotation : annotations) {
        if (!annotation.isEmpty()) {
            area |= annotation;
        }
    }
    return area & workspace;
}

} // namespace CanvasGeometry
