// SPDX-License-Identifier: GPL-3.0-or-later

#include "flowlayout.h"
#include "utils/flowpacking.h"

#include <QWidget>

#include <algorithm>

FlowLayout::FlowLayout(QWidget* parent, int spacing)
  : QLayout(parent)
{
    setSpacing(spacing);
    setContentsMargins(0, 0, 0, 0);
}

FlowLayout::~FlowLayout()
{
    while (QLayoutItem* item = takeAt(0)) {
        delete item;
    }
}

void FlowLayout::addItem(QLayoutItem* item)
{
    m_items.append(item);
}

int FlowLayout::count() const
{
    return static_cast<int>(m_items.size());
}

QLayoutItem* FlowLayout::itemAt(int index) const
{
    return m_items.value(index);
}

QLayoutItem* FlowLayout::takeAt(int index)
{
    if (index < 0 || index >= m_items.size()) {
        return nullptr;
    }
    return m_items.takeAt(index);
}

Qt::Orientations FlowLayout::expandingDirections() const
{
    return {};
}

bool FlowLayout::hasHeightForWidth() const
{
    return true;
}

int FlowLayout::heightForWidth(int width) const
{
    return arrange(QRect(0, 0, width, 0), false);
}

QSize FlowLayout::minimumSize() const
{
    // As narrow as the widest single item: anything wider wraps
    QSize size;
    for (const QLayoutItem* item : m_items) {
        if (!item->isEmpty()) {
            size = size.expandedTo(item->minimumSize());
        }
    }
    const QMargins margins = contentsMargins();
    return size + QSize(margins.left() + margins.right(),
                        margins.top() + margins.bottom());
}

QSize FlowLayout::sizeHint() const
{
    // Everything on one row is the preference; heightForWidth covers the rest
    int width = 0;
    int height = 0;
    int visible = 0;
    for (const QLayoutItem* item : m_items) {
        if (item->isEmpty()) {
            continue;
        }
        const QSize hint = item->sizeHint();
        width += hint.width();
        height = std::max(height, hint.height());
        ++visible;
    }
    if (visible > 1) {
        width += spacing() * (visible - 1);
    }
    const QMargins margins = contentsMargins();
    return QSize(width + margins.left() + margins.right(),
                 height + margins.top() + margins.bottom());
}

void FlowLayout::setGeometry(const QRect& rect)
{
    QLayout::setGeometry(rect);
    arrange(rect, true);
}

int FlowLayout::arrange(const QRect& rect, bool apply) const
{
    const QRect area = rect.marginsRemoved(contentsMargins());

    // Hidden items take no slot, or a hidden tool would leave a gap
    QVector<QLayoutItem*> shown;
    QVector<QSize> sizes;
    for (QLayoutItem* item : m_items) {
        if (!item->isEmpty()) {
            shown.append(item);
            sizes.append(item->sizeHint());
        }
    }
    const FlowPacking::Result packed =
      FlowPacking::pack(sizes, area.width(), spacing(), spacing());
    if (apply) {
        for (int i = 0; i < shown.size(); ++i) {
            shown.at(i)->setGeometry(
              packed.rects.at(i).translated(area.topLeft()));
        }
    }
    const QMargins margins = contentsMargins();
    return packed.height + margins.top() + margins.bottom();
}
