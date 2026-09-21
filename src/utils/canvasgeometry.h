// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QtGlobal>

/**
 * @brief Zoom arithmetic for the editor canvas.
 *
 * A free function on plain values so the stepping, clamping and the snap back
 * to actual size can be tested without constructing a widget, the same way
 * ToolSizeWheel keeps the wheel's rate limiting testable.
 */
namespace CanvasGeometry {

// Display scale limits. Shared with the toolbar so the two cannot drift.
inline constexpr qreal MinZoom = 0.1;
inline constexpr qreal MaxZoom = 8.0;
// One wheel notch multiplies or divides by this.
inline constexpr qreal ZoomFactor = 1.25;

// Positive notches zoom in, negative zoom out. The result is clamped to
// [MinZoom, MaxZoom] and lands exactly on 1.0 whenever a step would cross it,
// so the user can always get back to actual size with the wheel alone.
qreal zoomStep(qreal current, int notches);

} // namespace CanvasGeometry
