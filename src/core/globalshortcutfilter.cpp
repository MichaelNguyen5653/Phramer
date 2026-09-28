// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#include "globalshortcutfilter.h"
#include "core/flameshot.h"

#include <qt_windows.h>

GlobalShortcutFilter::GlobalShortcutFilter(QObject* parent)
  : QObject(parent)
{
    // Forced Print Screen
    if (RegisterHotKey(NULL, 1, 0, VK_SNAPSHOT)) {
        // ok - capture screen
    }

    if (RegisterHotKey(NULL, 2, MOD_SHIFT, VK_SNAPSHOT)) {
        // ok - show screenshots history
    }
}

bool GlobalShortcutFilter::nativeEventFilter(const QByteArray& eventType,
                                             void* message,
                                             qintptr* result)
{
    Q_UNUSED(eventType)
    Q_UNUSED(result)

    MSG* msg = static_cast<MSG*>(message);
    if (msg->message == WM_HOTKEY) {
        // TODO: this is just a temporary workaround; proper global
        // support would need custom shortcuts defined by the user.
        const quint32 keycode = HIWORD(msg->lParam);
        const quint32 modifiers = LOWORD(msg->lParam);
#ifdef ENABLE_IMGUR
        // Show screenshots history
        if (VK_SNAPSHOT == keycode && MOD_SHIFT == modifiers) {
            Flameshot::instance()->history();
            return true;
        }
#endif
        // Capture screen
        if (VK_SNAPSHOT == keycode && 0 == modifiers) {
            m_printHotkeyTime = GetTickCount64();
            Flameshot::instance()->requestCapture(
              CaptureRequest(CaptureRequest::GRAPHICAL_MODE));
            return true;
        }
    }
    // When Windows owns Print Screen (its "open screen capture" setting), it
    // launches ms-screenclip for the key, but not while one of Phramer's own
    // windows is in front: the editor, a pin or Settings get a plain key-up
    // instead, and without this the key does nothing there. Only messages
    // for this thread's windows reach this filter, so this never fires while
    // another application has the keyboard.
    if (msg->message == WM_KEYUP && msg->wParam == VK_SNAPSHOT) {
        // The key-up of a press that already arrived as our hotkey. A time
        // window rather than a flag: when the hotkey fires over another
        // application, that application gets the key-up and a flag would
        // swallow the next real press here.
        const ULONGLONG sinceHotkey = GetTickCount64() - m_printHotkeyTime;
        if (m_printHotkeyTime != 0 && sinceHotkey < 1500) {
            m_printHotkeyTime = 0;
            return false;
        }
        const auto held = [](int key) {
            return (GetKeyState(key) & 0x8000) != 0;
        };
        if (!held(VK_CONTROL) && !held(VK_SHIFT) && !held(VK_MENU) &&
            !held(VK_LWIN) && !held(VK_RWIN)) {
            Flameshot::instance()->requestCapture(
              CaptureRequest(CaptureRequest::GRAPHICAL_MODE));
            return true;
        }
    }
    return false; // Forward event to Qt
}
