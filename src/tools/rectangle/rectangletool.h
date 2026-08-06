// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#pragma once

#include "tools/abstracttwopointtool.h"

#include <QPainterPath>

class RectangleTool : public AbstractTwoPointTool
{
    Q_OBJECT
public:
    explicit RectangleTool(QObject* parent = nullptr);

    QIcon icon(const QColor& background, bool inEditor) const override;
    QString name() const override;
    QString description() const override;
    QWidget* configurationWidget() override;

    CaptureTool* copy(QObject* parent = nullptr) override;
    void process(QPainter& painter, const QPixmap& pixmap) override;

protected:
    void copyParams(const RectangleTool* from, RectangleTool* to);
    CaptureTool::Type type() const override;

public slots:
    void drawStart(const CaptureContext& context) override;
    void pressed(CaptureContext& context) override;

private slots:
    void setFillMode(int mode);

private:
    enum class FillMode
    {
        Solid = 0,
        Highlighter = 1,
    };

    // Rounded outline shared by both fill modes, so they cover exactly the
    // same area
    QPainterPath roundedPath() const;
    // The same area again, but valid at size 0 where the solid mode falls
    // back to a plain rectangle
    QPainterPath highlightPath() const;

    FillMode m_fillMode = FillMode::Solid;
    // Highlight border width, snapshotted at drawStart. See HighlightStyle.
    int m_outlineWidth = 2;
};
