// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QtGlobal>

/**
 * @brief Turns a wheel event's vertical delta into a tool size step.
 *
 * A mouse wheel reports one notch at a time, usually 120 and never much less.
 * A touchpad reports a stream of much smaller deltas, typically 2-8, so
 * treating each one as a step makes the size race away untouchably fast.
 * Touchpad deltas are therefore rate limited to one step per window.
 *
 * The capture overlay and the editor canvas share this so the gesture feels
 * the same on both, and so the heuristic has one home rather than two copies
 * that drift.
 */
namespace ToolSizeWheel {

// One notch, in the units QWheelEvent::angleDelta() reports
constexpr int WheelThreshold = 60;
// Smallest gap between two accepted touchpad steps, in milliseconds
constexpr qint64 TouchpadIntervalMs = 200;

struct Step
{
    // -1, 0 or +1. Never larger: a fast flick reports several notches at once
    // and the size must not leap by that much.
    int delta = 0;
    // False when a touchpad event arrived inside the rate limit window. The
    // caller should consume the event and do nothing.
    bool accepted = false;
    // True for a real wheel notch, false for a touchpad delta. Exposed
    // because only wheel notches are offered to the active tool first, which
    // is the behaviour the capture overlay has always had.
    bool fromWheel = false;
};

/**
 * @param angleDeltaY  QWheelEvent::angleDelta().y()
 * @param nowMs        current time in milliseconds
 * @param lastTouchpadMs  timestamp of the last accepted touchpad step;
 *                        updated in place when one is accepted here
 */
Step evaluate(int angleDeltaY, qint64 nowMs, qint64& lastTouchpadMs);

} // namespace ToolSizeWheel
