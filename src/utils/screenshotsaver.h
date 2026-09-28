// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#pragma once

#include <QString>
#include <QWidget>

class QPixmap;

// `savedPath`, when given, receives the file actually written: `path` may
// be a folder or a pattern
bool saveToFilesystem(const QPixmap& capture,
                      const QString& path,
                      const QString& messagePrefix = "",
                      QString* savedPath = nullptr);
QString ShowSaveFileDialog(const QString& title, const QString& directory);
void saveToClipboardMime(const QPixmap& capture, const QString& imageType);
void saveToClipboard(const QPixmap& capture);
// GNOME Wayland: keeps the widget alive until clipboard data is fetched
bool saveToClipboardGnomeWorkaround(const QPixmap& pixmap, QWidget* keepAlive);
bool saveToFilesystemGUI(const QPixmap& capture);
