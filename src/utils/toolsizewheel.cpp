// SPDX-License-Identifier: GPL-3.0-or-later

#include "toolsizewheel.h"

namespace ToolSizeWheel {

Step evaluate(int angleDeltaY, qint64 nowMs, qint64& lastTouchpadMs)
{
    Step step;

    if (qAbs(angleDeltaY) >= WheelThreshold) {
        step.fromWheel = true;
        step.accepted = true;
        step.delta = qMax(qMin(angleDeltaY / WheelThreshold, 1), -1);
        return step;
    }

    if ((nowMs - lastTouchpadMs) <= TouchpadIntervalMs) {
        return step;
    }

    lastTouchpadMs = nowMs;
    step.accepted = true;
    if (angleDeltaY > 0) {
        step.delta = 1;
    } else if (angleDeltaY < 0) {
        step.delta = -1;
    }
    return step;
}

} // namespace ToolSizeWheel
