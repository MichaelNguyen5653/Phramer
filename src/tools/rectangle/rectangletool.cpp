// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#include "rectangletool.h"

#include "tools/highlightstyle.h"
#include "utils/confighandler.h"

#include <QComboBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPainterPath>
#include <QSpinBox>
#include <cmath>

namespace {

const int MinFillMode = 0;
const int MaxFillMode = 1;

bool isValidFillMode(int mode)
{
    return mode >= MinFillMode && mode <= MaxFillMode;
}

} // unnamed namespace

RectangleTool::RectangleTool(QObject* parent)
  : AbstractTwoPointTool(parent)
{
    const int configuredFillMode = ConfigHandler().rectangleFillMode();
    if (isValidFillMode(configuredFillMode)) {
        m_fillMode = static_cast<FillMode>(configuredFillMode);
    }

    m_supportsDiagonalAdj = true;
}

QIcon RectangleTool::icon(const QColor& background, bool inEditor) const
{
    Q_UNUSED(inEditor)
    return QIcon(iconPath(background) + "square.svg");
}
QString RectangleTool::name() const
{
    return tr("Rectangle");
}

CaptureTool::Type RectangleTool::type() const
{
    return CaptureTool::TYPE_RECTANGLE;
}

QString RectangleTool::description() const
{
    return tr("Set the Rectangle as the paint tool");
}

QWidget* RectangleTool::configurationWidget()
{
    auto* widget = new QWidget();
    auto* layout = new QHBoxLayout(widget);
    auto* label = new QLabel(tr("Fill:"), widget);
    auto* modeSelector = new QComboBox(widget);

    modeSelector->addItem(tr("Solid"));
    modeSelector->addItem(tr("Highlighter"));
    modeSelector->setCurrentIndex(static_cast<int>(m_fillMode));
    connect(modeSelector,
            qOverload<int>(&QComboBox::currentIndexChanged),
            this,
            &RectangleTool::setFillMode);

    layout->addWidget(label);
    layout->addWidget(modeSelector);

    auto* borderLabel = new QLabel(tr("Border:"), widget);
    auto* borderSpin = new QSpinBox(widget);
    borderSpin->setRange(0, HighlightStyle::MaxOutlineWidth);
    borderSpin->setValue(ConfigHandler().highlightOutlineWidth());
    borderSpin->setToolTip(tr("Outline drawn around a highlight so it stays "
                              "visible on dark captures. 0 removes it."));
    connect(borderSpin, &QSpinBox::valueChanged, this, [](int value) {
        ConfigHandler().setHighlightOutlineWidth(value);
    });
    layout->addWidget(borderLabel);
    layout->addWidget(borderSpin);

    return widget;
}

void RectangleTool::setFillMode(int mode)
{
    if (!isValidFillMode(mode)) {
        mode = static_cast<int>(FillMode::Solid);
    }
    m_fillMode = static_cast<FillMode>(mode);
    ConfigHandler().setRectangleFillMode(mode);
}

CaptureTool* RectangleTool::copy(QObject* parent)
{
    auto* tool = new RectangleTool(parent);
    copyParams(this, tool);
    return tool;
}

void RectangleTool::copyParams(const RectangleTool* from, RectangleTool* to)
{
    AbstractTwoPointTool::copyParams(from, to);
    to->m_fillMode = from->m_fillMode;
    to->m_outlineWidth = from->m_outlineWidth;
}

QPainterPath RectangleTool::roundedPath() const
{
    QPainterPath path;
    int offset = size() <= 1 ? 1 : static_cast<int>(round(size() / 2 + 0.5));
    path.addRoundedRect(
      QRectF(std::min(points().first.x(), points().second.x()) - offset,
             std::min(points().first.y(), points().second.y()) - offset,
             std::abs(points().first.x() - points().second.x()) + offset * 2,
             std::abs(points().first.y() - points().second.y()) + offset * 2),
      size(),
      size());
    return path;
}

QPainterPath RectangleTool::highlightPath() const
{
    if (size() != 0) {
        return roundedPath();
    }
    // At size 0 the solid mode draws a plain rect, so the highlight has to
    // cover exactly the same area
    QPainterPath path;
    path.addRect(QRect(points().first, points().second).normalized());
    return path;
}

void RectangleTool::process(QPainter& painter, const QPixmap& pixmap)
{
    Q_UNUSED(pixmap)

    if (m_fillMode == FillMode::Highlighter) {
        const QPainterPath path = highlightPath();
        {
            // The fill is never stroked inside this scope: an outline drawn
            // under multiply would composite over the fill it sits on and
            // darken the border, which a marker stroke never does.
            HighlightStyle::PainterState highlight(painter);
            painter.fillPath(path, color());
        }

        // See HighlightStyle: multiply cannot lighten, so the fill alone is
        // invisible on a dark capture. Drawn outside the highlight scope so
        // it composites normally and does not darken its own border.
        if (m_outlineWidth > 0) {
            const QPen originalPen = painter.pen();
            const QBrush originalBrush = painter.brush();
            painter.setPen(QPen(color(),
                                m_outlineWidth,
                                Qt::SolidLine,
                                Qt::SquareCap,
                                Qt::RoundJoin));
            painter.setBrush(Qt::NoBrush);
            painter.drawPath(path);
            painter.setPen(originalPen);
            painter.setBrush(originalBrush);
        }
        return;
    }

    QPen orig_pen = painter.pen();
    QBrush orig_brush = painter.brush();
    painter.setPen(
      QPen(color(), size(), Qt::SolidLine, Qt::SquareCap, Qt::RoundJoin));
    painter.setBrush(QBrush(color()));
    if (size() == 0) {
        painter.drawRect(QRect(points().first, points().second));
    } else {
        painter.fillPath(roundedPath(), color());
    }
    painter.setPen(orig_pen);
    painter.setBrush(orig_brush);
}

void RectangleTool::drawStart(const CaptureContext& context)
{
    // Snapshotted here, not read while painting: a placed highlight keeps the
    // border it was drawn with even if the setting changes afterwards
    m_outlineWidth = ConfigHandler().highlightOutlineWidth();
    AbstractTwoPointTool::drawStart(context);
    onSizeChanged(context.toolSize);
}

void RectangleTool::pressed(CaptureContext& context)
{
    Q_UNUSED(context)
}
