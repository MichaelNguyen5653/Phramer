// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#include "globalvalues.h"

#include <QApplication>
#include <QFontMetrics>
#include <QIcon>
#if defined(Q_OS_MACOS)
#include <QOperatingSystemVersion>
#endif

int GlobalValues::buttonBaseSize()
{
    return QFontMetrics(qApp->font()).lineSpacing() * 2.2;
}

QString GlobalValues::versionInfo()
{
    return QStringLiteral("Phramer " APP_VERSION " (" FLAMESHOT_GIT_HASH ")"
                          "\nCompiled with Qt " QT_VERSION_STR);
}

const QIcon& GlobalValues::appIcon()
{
    static const QIcon icon = []() {
        QIcon built;
        for (int size : { 16, 24, 32, 48, 64, 128, 256, 512 }) {
            built.addFile(QStringLiteral(":img/app/appicon-%1.png").arg(size),
                          QSize(size, size));
        }
        return built;
    }();
    return icon;
}

QString GlobalValues::iconPath()
{
    return { ":img/app/appicon-256.png" };
}

QString GlobalValues::iconPathPNG()
{
    return { ":img/app/appicon-256.png" };
}

QString GlobalValues::trayIconPath()
{
    // The tray draws at 16 or 24 px depending on DPI; the small renders are
    // the ones that were made to survive it
    return { ":img/app/appicon-32.png" };
}
