// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QRect>
#include <QSize>
#include <QVector>
#include <QtGlobal>

/**
 * @brief Zoom and workspace arithmetic for the editor canvas.
 *
 * Free functions on plain values so they can be tested without constructing
 * a widget, the same way ToolSizeWheel keeps the wheel's rate limiting
 * testable.
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

// The image floats on a larger workspace so annotations can reach past it.
// The margin on each side, in device-independent pixels, is a share of the
// image's larger side within these bounds.
inline constexpr int MinWorkspaceMargin = 160;
inline constexpr int MaxWorkspaceMargin = 800;

// That margin, nudged up until it is a whole number of physical pixels at
// devicePixelRatio: the image then sits on the pixel grid and is copied, not
// resampled, which is what keeps an unannotated export identical to the
// capture.
int workspaceMargin(const QSize& imageSize, qreal devicePixelRatio);

// A device-independent rectangle in physical pixels, rounded outward so
// nothing it covers is cut off
QRect toPhysical(const QRect& logical, qreal devicePixelRatio);

// What an export covers, in physical pixels: the image, grown to take in
// every annotation that reaches past it, and never past the workspace
QRect exportRect(const QRect& image,
                 const QVector<QRect>& annotations,
                 const QRect& workspace);

} // namespace CanvasGeometry
