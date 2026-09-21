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

} // namespace CanvasGeometry
