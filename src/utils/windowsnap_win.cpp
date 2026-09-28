// SPDX-License-Identifier: GPL-3.0-or-later

#include "windowsnap.h"

#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

#include <dwmapi.h>

namespace {

struct Enumeration
{
    DWORD ownProcess = 0;
    QVector<QRect> windows;
};

bool isCloaked(HWND window)
{
    // Suspended UWP apps and windows on other virtual desktops are cloaked:
    // invisible, yet IsWindowVisible still says yes
    BOOL cloaked = FALSE;
    return SUCCEEDED(DwmGetWindowAttribute(
             window, DWMWA_CLOAKED, &cloaked, sizeof(cloaked))) &&
           cloaked != FALSE;
}

BOOL CALLBACK collect(HWND window, LPARAM data)
{
    auto* state = reinterpret_cast<Enumeration*>(data);

    DWORD process = 0;
    GetWindowThreadProcessId(window, &process);
    if (process == state->ownProcess || IsWindowVisible(window) == FALSE ||
        IsIconic(window) != FALSE || isCloaked(window)) {
        return TRUE;
    }
    // Click-through windows (overlays, notification hosts) are invisible to
    // the mouse, so they must not be what the mouse picks either
    if ((GetWindowLongW(window, GWL_EXSTYLE) & WS_EX_TRANSPARENT) != 0) {
        return TRUE;
    }

    // The extended frame bounds are the window as drawn. GetWindowRect
    // includes the invisible resize border and shadow, which would put a
    // band of the window behind into the capture.
    RECT bounds;
    if (FAILED(DwmGetWindowAttribute(
          window, DWMWA_EXTENDED_FRAME_BOUNDS, &bounds, sizeof(bounds))) &&
        GetWindowRect(window, &bounds) == FALSE) {
        return TRUE;
    }
    const QRect rect(
      QPoint(bounds.left, bounds.top),
      QSize(bounds.right - bounds.left, bounds.bottom - bounds.top));
    if (!rect.isEmpty()) {
        state->windows.append(rect);
    }
    return TRUE;
}

} // namespace

namespace WindowSnap {

QVector<QRect> visibleWindows()
{
    Enumeration state;
    state.ownProcess = GetCurrentProcessId();
    // EnumWindows reports top-level windows in z-order, topmost first,
    // which is the order windowAt() relies on
    EnumWindows(collect, reinterpret_cast<LPARAM>(&state));
    return state.windows;
}

} // namespace WindowSnap
