// SPDX-License-Identifier: GPL-3.0-or-later

#include "screencoordinates.h"

#if defined(Q_OS_WIN)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#endif

QPoint nativeWindowOrigin(WId window, const QPoint& fallback)
{
#if defined(Q_OS_WIN)
    RECT rect;
    if (window != 0 &&
        GetWindowRect(reinterpret_cast<HWND>(window), &rect) != 0) {
        return { static_cast<int>(rect.left), static_cast<int>(rect.top) };
    }
#else
    Q_UNUSED(window)
#endif
    return fallback;
}
