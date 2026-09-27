// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#pragma once

#include "tools/capturetool.h"

#include <QObject>

class CaptureTool;

class ToolFactory : public QObject
{
    Q_OBJECT

public:
    explicit ToolFactory(QObject* parent = nullptr);

    ToolFactory(const ToolFactory&) = delete;
    ToolFactory& operator=(const ToolFactory&) = delete;

    CaptureTool* CreateTool(CaptureTool::Type t, QObject* parent = nullptr);

    // One word per tool, for the labels under buttons on the capture overlay
    // and in the editor. Kept apart from CaptureTool::name(), which is the
    // longer phrase used in tooltips and settings.
    static QString shortName(CaptureTool::Type t);
    // shortName() with its key appended, "Pencil (P)", but only for keys
    // pressed on their own. A modifier combination such as Ctrl+Shift+Z
    // would make one label much wider than the rest, and on the capture
    // overlay the widest label sets the spacing of every button. Several
    // keys may be given separated by "/", as the editor's shape button has.
    // name overrides shortName() where a surface calls the tool differently.
    static QString labelWithShortcut(CaptureTool::Type t,
                                     const QString& shortcut,
                                     const QString& name = QString());
};
