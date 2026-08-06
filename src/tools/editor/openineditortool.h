// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/abstractactiontool.h"

/**
 * @brief Sends the finished capture to the standalone editor.
 *
 * Like the other terminal actions, this only tags the request; the routing
 * happens in Flameshot::exportCapture once the capture widget has been torn
 * down and the pixmap is final.
 */
class OpenInEditorTool : public AbstractActionTool
{
    Q_OBJECT
public:
    explicit OpenInEditorTool(QObject* parent = nullptr);

    bool closeOnButtonPressed() const override;

    QIcon icon(const QColor& background, bool inEditor) const override;
    QString name() const override;
    QString description() const override;

    CaptureTool* copy(QObject* parent = nullptr) override;

protected:
    CaptureTool::Type type() const override;

public slots:
    void pressed(CaptureContext& context) override;
};
