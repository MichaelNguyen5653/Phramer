// SPDX-License-Identifier: GPL-3.0-or-later

#include "shapetool.h"

#include "tools/highlightstyle.h"
#include "utils/confighandler.h"
#include "utils/pathinfo.h"

#include <QButtonGroup>
#include <QGridLayout>
#include <QGuiApplication>
#include <QLabel>
#include <QMenu>
#include <QPainter>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QToolButton>
#include <QWidgetAction>
#include <cmath>

namespace {

struct VariantButton
{
    int value;
    const char* icon;
    const char* label;
};

const VariantButton KindButtons[] = {
    { static_cast<int>(ShapeTool::Kind::Square), "square-outline.svg", "Square" },
    { static_cast<int>(ShapeTool::Kind::Circle), "circle-outline.svg", "Circle" },
};

const VariantButton StyleButtons[] = {
    { static_cast<int>(ShapeTool::Style::Hollow), "square-outline.svg", "Hollow" },
    { static_cast<int>(ShapeTool::Style::Filled), "square.svg", "Filled" },
    { static_cast<int>(ShapeTool::Style::Highlight),
      "square-highlight.svg",
      "Highlight" },
};

// The picker is drawn with the menu's own palette, not the capture overlay's
// tool colour, so it needs the icon variant that contrasts with the menu
QString menuIconPath()
{
    const QColor background =
      QGuiApplication::palette().color(QPalette::Window);
    return background.lightness() < 128 ? PathInfo::whiteIconPath()
                                        : PathInfo::blackIconPath();
}

QToolButton* makeVariantButton(QWidget* parent,
                               const VariantButton& variant,
                               const QString& iconDir)
{
    auto* button = new QToolButton(parent);
    button->setCheckable(true);
    button->setAutoRaise(true);
    button->setIcon(QIcon(iconDir + QLatin1String(variant.icon)));
    button->setText(QObject::tr(variant.label));
    button->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    button->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    return button;
}

} // unnamed namespace

ShapeTool::ShapeTool(QObject* parent)
  : AbstractTwoPointTool(parent)
  , m_kind(configuredKind())
  , m_style(configuredStyle())
  , m_outlineWidth(ConfigHandler().highlightOutlineWidth())
{
    m_supportsDiagonalAdj = true;
}

ShapeTool::Kind ShapeTool::configuredKind()
{
    return static_cast<Kind>(ConfigHandler().shapeKind());
}

ShapeTool::Style ShapeTool::configuredStyle()
{
    return static_cast<Style>(ConfigHandler().shapeStyle());
}

QString ShapeTool::iconName(Kind kind, Style style)
{
    const bool square = kind == Kind::Square;
    switch (style) {
        case Style::Hollow:
            return square ? QStringLiteral("square-outline.svg")
                          : QStringLiteral("circle-outline.svg");
        case Style::Filled:
            return square ? QStringLiteral("square.svg")
                          : QStringLiteral("circle.svg");
        case Style::Highlight:
        default:
            return square ? QStringLiteral("square-highlight.svg")
                          : QStringLiteral("circle-highlight.svg");
    }
}

QIcon ShapeTool::icon(const QColor& background, bool inEditor) const
{
    Q_UNUSED(inEditor)
    // Read the picker rather than the cached members: the toolbar's tool
    // instance outlives any number of picker changes
    return QIcon(iconPath(background) +
                 iconName(configuredKind(), configuredStyle()));
}

QString ShapeTool::name() const
{
    return tr("Shape");
}

CaptureTool::Type ShapeTool::type() const
{
    return CaptureTool::TYPE_SHAPE;
}

QString ShapeTool::description() const
{
    return tr("Draw a square or circle, hollow, filled or as a highlight");
}

bool ShapeTool::hasOptionsMenu() const
{
    return true;
}

QMenu* ShapeTool::optionsMenu(QWidget* parent)
{
    return buildMenu(parent, {});
}

QMenu* ShapeTool::buildMenu(QWidget* parent,
                            const std::function<void()>& onChanged)
{
    auto* menu = new QMenu(parent);
    auto* panel = new QWidget(menu);
    auto* layout = new QGridLayout(panel);
    layout->setContentsMargins(8, 6, 8, 6);

    const QString iconDir = menuIconPath();

    // Two independent controls rather than one flat list of combinations, so
    // changing the outline style does not mean re-picking the shape
    auto* kindGroup = new QButtonGroup(panel);
    auto* styleGroup = new QButtonGroup(panel);
    kindGroup->setExclusive(true);
    styleGroup->setExclusive(true);

    layout->addWidget(new QLabel(QObject::tr("Shape"), panel), 0, 0);
    int column = 1;
    for (const VariantButton& variant : KindButtons) {
        QToolButton* button = makeVariantButton(panel, variant, iconDir);
        kindGroup->addButton(button, variant.value);
        layout->addWidget(button, 0, column++);
    }

    layout->addWidget(new QLabel(QObject::tr("Style"), panel), 1, 0);
    column = 1;
    for (const VariantButton& variant : StyleButtons) {
        QToolButton* button = makeVariantButton(panel, variant, iconDir);
        styleGroup->addButton(button, variant.value);
        layout->addWidget(button, 1, column++);
    }

    // The border only affects the highlight style, but it lives here rather
    // than in a separate place because this menu is where the highlight is
    // chosen in the first place
    auto* borderLabel = new QLabel(QObject::tr("Border"), panel);
    auto* borderSpin = new QSpinBox(panel);
    borderSpin->setRange(0, HighlightStyle::MaxOutlineWidth);
    borderSpin->setValue(ConfigHandler().highlightOutlineWidth());
    borderSpin->setToolTip(
      QObject::tr("Outline drawn around a highlight so it stays visible on "
                  "dark captures. 0 removes it."));
    layout->addWidget(borderLabel, 2, 0);
    layout->addWidget(borderSpin, 2, 1, 1, 2);

    const auto syncFromConfig = [kindGroup, styleGroup, borderSpin]() {
        if (QAbstractButton* button =
              kindGroup->button(ConfigHandler().shapeKind())) {
            button->setChecked(true);
        }
        if (QAbstractButton* button =
              styleGroup->button(ConfigHandler().shapeStyle())) {
            button->setChecked(true);
        }
        QSignalBlocker blocker(borderSpin);
        borderSpin->setValue(ConfigHandler().highlightOutlineWidth());
    };
    syncFromConfig();
    // The overlay and the editor each own a picker; re-reading on open keeps
    // whichever is shown second in step with the other
    QObject::connect(menu, &QMenu::aboutToShow, panel, syncFromConfig);

    QObject::connect(
      kindGroup, &QButtonGroup::idClicked, panel, [onChanged](int id) {
          ConfigHandler().setShapeKind(id);
          if (onChanged) {
              onChanged();
          }
      });
    QObject::connect(
      styleGroup, &QButtonGroup::idClicked, panel, [onChanged](int id) {
          ConfigHandler().setShapeStyle(id);
          if (onChanged) {
              onChanged();
          }
      });
    QObject::connect(borderSpin, &QSpinBox::valueChanged, panel, [](int value) {
        ConfigHandler().setHighlightOutlineWidth(value);
    });

    // A QWidgetAction keeps the panel open across picks; plain menu actions
    // would dismiss after the shape and force a second trip for the style
    auto* action = new QWidgetAction(menu);
    action->setDefaultWidget(panel);
    menu->addAction(action);
    return menu;
}

CaptureTool* ShapeTool::copy(QObject* parent)
{
    auto* tool = new ShapeTool(parent);
    copyParams(this, tool);
    return tool;
}

void ShapeTool::copyParams(const ShapeTool* from, ShapeTool* to)
{
    AbstractTwoPointTool::copyParams(from, to);
    to->m_kind = from->m_kind;
    to->m_style = from->m_style;
    to->m_outlineWidth = from->m_outlineWidth;
}

QPainterPath ShapeTool::filledPath() const
{
    const int offset =
      size() <= 1 ? 1 : static_cast<int>(round(size() / 2.0 + 0.5));
    const QRectF area(
      std::min(points().first.x(), points().second.x()) - offset,
      std::min(points().first.y(), points().second.y()) - offset,
      std::abs(points().first.x() - points().second.x()) + offset * 2,
      std::abs(points().first.y() - points().second.y()) + offset * 2);

    QPainterPath path;
    if (m_kind == Kind::Square) {
        path.addRoundedRect(area, size(), size());
    } else {
        path.addEllipse(area);
    }
    return path;
}

void ShapeTool::process(QPainter& painter, const QPixmap& pixmap)
{
    Q_UNUSED(pixmap)

    const QRect area(points().first, points().second);
    const QPen originalPen = painter.pen();
    const QBrush originalBrush = painter.brush();

    switch (m_style) {
        case Style::Hollow:
            // Stroke the plain shape, never the inflated path: the outline is
            // the whole annotation here, so its position must be exactly
            // where the user dragged
            painter.setPen(QPen(color(),
                                size(),
                                Qt::SolidLine,
                                Qt::SquareCap,
                                m_kind == Kind::Square ? Qt::MiterJoin
                                                       : Qt::RoundJoin));
            painter.setBrush(Qt::NoBrush);
            if (m_kind == Kind::Square) {
                painter.drawRect(area);
            } else {
                painter.drawEllipse(area);
            }
            break;

        case Style::Filled:
            if (size() == 0) {
                painter.setPen(QPen(color()));
                painter.setBrush(QBrush(color()));
                if (m_kind == Kind::Square) {
                    painter.drawRect(area);
                } else {
                    painter.drawEllipse(area);
                }
            } else {
                painter.fillPath(filledPath(), color());
            }
            break;

        case Style::Highlight: {
            const QPainterPath path = filledPath();
            {
                // Nothing is stroked inside this scope: an outline drawn
                // under multiply would composite over the fill it sits on
                // and darken its own border
                HighlightStyle::PainterState highlight(painter);
                painter.fillPath(path, color());
            }
            // Multiply cannot lighten, so the fill alone vanishes on a dark
            // capture. See HighlightStyle.
            if (m_outlineWidth > 0) {
                painter.setPen(QPen(color(),
                                    m_outlineWidth,
                                    Qt::SolidLine,
                                    Qt::SquareCap,
                                    Qt::RoundJoin));
                painter.setBrush(Qt::NoBrush);
                painter.drawPath(path);
            }
            break;
        }
    }

    painter.setPen(originalPen);
    painter.setBrush(originalBrush);
}

void ShapeTool::drawStart(const CaptureContext& context)
{
    // Taken at drag time rather than construction: the toolbar keeps one
    // instance alive for the whole capture, across any number of picks
    m_kind = configuredKind();
    m_style = configuredStyle();
    m_outlineWidth = ConfigHandler().highlightOutlineWidth();
    AbstractTwoPointTool::drawStart(context);
    onSizeChanged(context.toolSize);
}

void ShapeTool::pressed(CaptureContext& context)
{
    Q_UNUSED(context)
}
