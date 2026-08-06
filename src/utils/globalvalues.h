// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#pragma once

class QIcon;
class QString;

namespace GlobalValues {

int buttonBaseSize();
QString versionInfo();
QString iconPath();
QString iconPathPNG();
QString trayIconPath();

/**
 * @brief The application icon at every size that ships.
 *
 * A QIcon built from a single large PNG is downscaled on the fly wherever
 * Windows asks for 16 or 20 px - the taskbar, the title bar, Alt-Tab, the
 * tray - and a disc with a fine mark inside it turns to mush at that size.
 * Handing Qt each pre-rendered size lets it pick rather than resample.
 */
const QIcon& appIcon();
}
