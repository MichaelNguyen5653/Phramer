// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QPoint>
#include <QtGlobal>
#include <qwindowdefs.h>

/**
 * @brief Top-left of a native window in physical screen pixels.
 *
 * Qt widget geometry is in logical pixels, and when monitors run at
 * different scale factors there is no arithmetic that converts the logical
 * desktop into physical screen space — that is the same reason the capture
 * overlay uses one window per screen. Native APIs (UI Automation, DWM,
 * anything taking a POINT) work in physical coordinates, so a rectangle
 * handed to one of them has to be anchored to the window handle rather than
 * derived from Qt's geometry.
 *
 * `fallback` is returned on platforms with no native handle to ask.
 */
QPoint nativeWindowOrigin(WId window, const QPoint& fallback);
