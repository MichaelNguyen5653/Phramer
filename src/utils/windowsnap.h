// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QPoint>
#include <QRect>
#include <QVector>

/**
 * @brief Window capture: which window is under the mouse, and where it is on
 * the capture overlay.
 *
 * The windows are listed once, when the capture starts, so they match the
 * frozen screenshot rather than whatever has moved since. Their rectangles
 * are in physical screen pixels, the only space in which DWM reports a
 * window's visible frame; the overlay converts them through its own native
 * origin (see utils/screencoordinates.h) rather than through Qt's geometry,
 * which cannot express a mixed-DPI desktop.
 *
 * The hit test and the mapping are pure so they can be tested without a
 * desktop.
 */
namespace WindowSnap {

// The first rectangle containing `point`, or a null rectangle. `windows`
// is topmost first.
QRect windowAt(const QVector<QRect>& windows, const QPoint& point);

// A physical screen rectangle in the overlay's logical coordinates, clipped
// to `overlay`. Null when the window is not on this overlay's screen at all.
QRect toOverlay(const QRect& physical,
                const QPoint& overlayNativeOrigin,
                qreal devicePixelRatio,
                const QRect& overlay);

#if defined(Q_OS_WIN) || defined(_WIN32)
// Visible top-level windows of other processes, topmost first, as their
// visible frame (without the drop shadow) in physical screen pixels. DWM
// reports physical pixels only to a per-monitor DPI aware process, which Qt 6
// makes every application by default; a manifest that lowered it would skew
// these rectangles on scaled monitors.
QVector<QRect> visibleWindows();
#endif

} // namespace WindowSnap
