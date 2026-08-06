// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/abstractactiontool.h"

#include <QPixmap>
#include <QRect>

class OcrTool : public AbstractActionTool
{
    Q_OBJECT
public:
    explicit OcrTool(QObject* parent = nullptr);

    bool closeOnButtonPressed() const override;

    QIcon icon(const QColor& background, bool inEditor) const override;
    QString name() const override;
    QString description() const override;

    QWidget* widget() override;

    CaptureTool* copy(QObject* parent = nullptr) override;

protected:
    CaptureTool::Type type() const override;

public slots:
    void pressed(CaptureContext& context) override;

private:
    QPixmap m_capture;
    // Where that pixmap came from, in physical screen pixels, so the text
    // can also be asked for from the window that drew it
    QRect m_captureScreenRect;
};
