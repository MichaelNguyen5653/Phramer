// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/abstractactiontool.h"

/**
 * @brief Mode that selects and drags annotations already placed on the canvas.
 *
 * The tool draws nothing itself. Like TYPE_MOVESELECTION it exists so the
 * toolbar has a button and the shortcut has a name; CaptureWidget recognises
 * the type and routes presses to object picking instead of drawing.
 */
class MoveObjectTool : public AbstractActionTool
{
    Q_OBJECT
public:
    explicit MoveObjectTool(QObject* parent = nullptr);

    bool closeOnButtonPressed() const override;
    bool isSelectable() const override;

    QIcon icon(const QColor& background, bool inEditor) const override;
    QString name() const override;
    CaptureTool::Type type() const override;
    QString description() const override;

    CaptureTool* copy(QObject* parent = nullptr) override;

public slots:
    void pressed(CaptureContext& context) override;
};
