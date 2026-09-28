// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/abstractactiontool.h"

/**
 * @brief Saves the selection to the save folder and copies the file itself.
 *
 * Pasting a file into Teams, Outlook or Explorer attaches it with a name,
 * where pasting an image inlines and recompresses it. The export does the
 * work (CaptureRequest::COPY_FILE); this only asks for it.
 */
class CopyFileTool : public AbstractActionTool
{
    Q_OBJECT
public:
    explicit CopyFileTool(QObject* parent = nullptr);

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
