// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/ocr/ocrengine.h"

#include <QRect>

/**
 * @brief Read the real text out of the window under a screen rectangle.
 *
 * Screenshots of text are a lossy encoding of something the source
 * application still holds verbatim. Where that application exposes a UI
 * Automation text provider — terminals, browsers, editors, most native
 * controls — reading it back is exact, and sidesteps every failure mode
 * recognition has: small glyphs, antialiasing, and fonts whose `l` and `t`
 * differ by a few pixels.
 *
 * `screenRect` is in physical screen pixels; see nativeWindowOrigin() in
 * utils/screencoordinates.h for why Qt geometry cannot be used directly.
 * Windows belonging to this process are skipped, so the capture overlay
 * covering the screen does not hide the window underneath it.
 *
 * Only lines lying wholly inside the rectangle are returned. A line the
 * selection clipped has words that were never captured, so it is left out
 * and recognition answers for it instead. Password controls are never read.
 *
 * Returns Status::NoTextFound when there is no provider or no text, which
 * is the normal outcome for images, remote desktop sessions and games.
 * Blocks on cross-process calls, so call it off the GUI thread.
 */
OcrResult readWindowText(const QRect& screenRect);
