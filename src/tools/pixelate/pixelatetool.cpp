// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#include "pixelatetool.h"
#include "frostedfill.h"
#include "utils/confighandler.h"

#include <QApplication>
#include <QGraphicsBlurEffect>
#include <QGraphicsPixmapItem>
#include <QGraphicsScene>
#include <QImage>
#include <QPainter>

PixelateTool::PixelateTool(QObject* parent)
  : AbstractTwoPointTool(parent)
{}

QIcon PixelateTool::icon(const QColor& background, bool inEditor) const
{
    Q_UNUSED(inEditor)
    return QIcon(iconPath(background) + "pixelate.svg");
}

QString PixelateTool::name() const
{
    return tr("Pixelate");
}

CaptureTool::Type PixelateTool::type() const
{
    return CaptureTool::TYPE_PIXELATE;
}

QString PixelateTool::description() const
{
    return tr("Set Pixelate as the paint tool.");
}

QRect PixelateTool::boundingRect() const
{
    return QRect(points().first, points().second).normalized();
}

CaptureTool* PixelateTool::copy(QObject* parent)
{
    auto* tool = new PixelateTool(parent);
    copyParams(this, tool);
    return tool;
}

/**
 * Since pixelation does not protect the contents of the pixelated area
 * (see e.g. https://github.com/bishopfox/unredacter), the default is a
 * frosted fill built only from the pixels bordering the selected area. The
 * interior is not used as an input at all and hence can not be recovered.
 * The insecure setting keeps the real blur and pixelation.
 */
void PixelateTool::process(QPainter& painter, const QPixmap& pixmap)
{
    bool useInsecurePixelate = ConfigHandler().insecurePixelate();

    QRect selection = boundingRect().intersected(pixmap.rect());
    auto pixelRatio = pixmap.devicePixelRatio();
    QRect selectionScaled = QRect(selection.topLeft() * pixelRatio,
                                  selection.bottomRight() * pixelRatio);

    const auto width =
      static_cast<int>(selection.width() * (0.5 / qMax(1, size() + 1)));
    const auto height =
      static_cast<int>(selection.height() * (0.5 / qMax(1, size() + 1)));
    const auto effectSize = QSize(qMax(width, 1), qMax(height, 1));

    if (useInsecurePixelate) {
        if (size() <= 1) {
            auto* blur = new QGraphicsBlurEffect(this);
            blur->setBlurRadius(10);
            auto* item = new QGraphicsPixmapItem(pixmap.copy(selectionScaled));
            item->setGraphicsEffect(blur);

            QGraphicsScene scene;
            scene.addItem(item);

            scene.render(&painter, selection, QRectF());
            blur->setBlurRadius(12);
            // multiple repeat for make blur effect stronger
            scene.render(&painter, selection, QRectF());

        } else {
            auto pixmapPixelated = pixmap.copy(selectionScaled);
            pixmapPixelated = pixmapPixelated.scaled(
              effectSize, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
            pixmapPixelated =
              pixmapPixelated.scaled(selection.width(), selection.height());
            painter.drawImage(selection, pixmapPixelated.toImage());
        }
    } else {
        // Only the pixels around the box are read, so there is nothing
        // inside it to recover (see frostedFill). The rect is built from the
        // outer edges, the same span the painter covers when it draws into
        // selection: one pixel short would put the "outside" line inside.
        const QRect physical =
          QRect(
            QPoint(qRound(selection.left() * pixelRatio),
                   qRound(selection.top() * pixelRatio)),
            QPoint(qRound((selection.x() + selection.width()) * pixelRatio) - 1,
                   qRound((selection.y() + selection.height()) * pixelRatio) -
                     1))
            .intersected(pixmap.rect());
        const FrostedEdges edges = frostedEdges(pixmap, physical);
        // process() runs on every repaint for every placed object, and a
        // full-screen box is millions of pixels, so the fill is rebuilt only
        // when what it depends on changes
        if (m_fill.isNull() || m_fill.size() != physical.size() ||
            m_fillStrength != size() || m_fillEdges != edges) {
            m_fill = frostedFill(edges, physical.size(), size());
            m_fillStrength = size();
            m_fillEdges = edges;
        }
        painter.drawImage(selection, m_fill);

        // A faint light rim is what makes it read as glass rather than as a
        // smudge
        painter.save();
        painter.setRenderHint(QPainter::Antialiasing, false);
        painter.setBrush(Qt::NoBrush);
        painter.setPen(QPen(QColor(255, 255, 255, 40), 0));
        painter.drawRect(selection.adjusted(0, 0, -1, -1));
        painter.restore();
    }
}

void PixelateTool::drawSearchArea(QPainter& painter, const QPixmap& pixmap)
{
    Q_UNUSED(pixmap)
    painter.fillRect(boundingRect(), QBrush(Qt::black));
}

void PixelateTool::paintMousePreview(QPainter& painter,
                                     const CaptureContext& context)
{
    Q_UNUSED(context)
    Q_UNUSED(painter)
}

void PixelateTool::pressed(CaptureContext& context)
{
    Q_UNUSED(context)
}
