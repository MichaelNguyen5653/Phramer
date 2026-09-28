// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#if defined(Q_OS_WIN) || defined(_WIN32)

#include <functional>

/**
 * @brief Handing video capture to the Windows Snipping Tool.
 *
 * Phramer records nothing itself. Snipping Tool already records the screen,
 * is maintained by Microsoft, and is what IT has already approved.
 *
 * Its documented recording link, ms-screenclip://capture/video, only answers
 * apps with a package identity. The link Windows itself uses for Win+Shift+R
 * answers anyone, and opens the recording overlay directly, provided it is
 * launched at Snipping Tool's package and not left to the ms-screenclip
 * handler: Phramer can be that handler, and the request would come straight
 * back. Never launch an ms-screenclip link without that target.
 */
namespace SnippingTool {

// Installed for this user and not removed by IT. A block by AppLocker or
// App Control still reads as installed; launch() reports that case.
bool isAvailable();

// Starts Snipping Tool, or brings it forward. Returns false when Windows
// refused, which on a managed machine usually means policy blocked it.
bool launch();

// Opens Snipping Tool's recording overlay, as Win+Shift+R does. Falls back
// to launch() if Windows would not open the link. Blocks until Windows has
// answered, so it is for the short-lived link-handling process only.
bool startRecording();

// The same without blocking, for the GUI thread. done runs on a worker
// thread with whether anything was opened; marshal it back before touching
// widgets.
void startRecordingAsync(std::function<void(bool)> done);

// For the application's aboutToQuit: stops pending startRecordingAsync()
// callbacks from running and waits briefly for any already running.
void finishPendingLaunches();

} // namespace SnippingTool

#endif
