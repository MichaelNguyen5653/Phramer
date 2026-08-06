// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/editor/editorfilmstrip.h"

#include <QScrollBar>
#include <QScroller>
#include <QWheelEvent>

namespace {
constexpr int ThumbnailWidth = 148;
constexpr int ThumbnailHeight = 84;
// Room for the frame, the ordinal label and the item's own margins
constexpr int StripHeight = 132;
} // namespace

EditorFilmstrip::EditorFilmstrip(QWidget* parent)
  : QListWidget(parent)
{
    setViewMode(QListView::IconMode);
    setFlow(QListView::LeftToRight);
    setWrapping(false);
    setMovement(QListView::Static);
    setResizeMode(QListView::Adjust);
    setSelectionMode(QAbstractItemView::SingleSelection);
    setIconSize(QSize(ThumbnailWidth, ThumbnailHeight));
    setGridSize(QSize(ThumbnailWidth + 16, ThumbnailHeight + 28));
    setUniformItemSizes(true);
    setTextElideMode(Qt::ElideNone);
    setWordWrap(false);
    setVerticalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    setHorizontalScrollBarPolicy(Qt::ScrollBarAsNeeded);
    // Pixel scrolling keeps flicking and wheel steps smooth instead of
    // jumping a whole thumbnail at a time
    setHorizontalScrollMode(QAbstractItemView::ScrollPerPixel);
    setFixedHeight(StripHeight);

    // Touch and trackpad flick. Left-button dragging is deliberately not
    // grabbed, since that would swallow click-to-select.
    QScroller::grabGesture(viewport(), QScroller::TouchGesture);

    connect(this, &QListWidget::currentRowChanged, this, [this](int row) {
        if (m_updatingSelection || row < 0) {
            return;
        }
        emit imageActivated(row);
    });
}

void EditorFilmstrip::appendImage(const QPixmap& image)
{
    auto* item = new QListWidgetItem(makeThumbnail(image), QString(), this);
    item->setTextAlignment(Qt::AlignHCenter | Qt::AlignBottom);
    renumber();
}

void EditorFilmstrip::updateImage(int index, const QPixmap& image)
{
    QListWidgetItem* entry = item(index);
    if (entry) {
        entry->setIcon(makeThumbnail(image));
    }
}

void EditorFilmstrip::removeImage(int index)
{
    delete takeItem(index);
    renumber();
}

void EditorFilmstrip::setActiveIndex(int index)
{
    if (index < 0 || index >= count() || index == currentRow()) {
        return;
    }
    // Guard the signal so programmatic sync from Prev/Next does not bounce
    // back into the window as a fresh navigation request
    m_updatingSelection = true;
    setCurrentRow(index);
    m_updatingSelection = false;
    scrollToItem(item(index), QAbstractItemView::EnsureVisible);
}

void EditorFilmstrip::wheelEvent(QWheelEvent* event)
{
    const int delta = event->angleDelta().y() != 0 ? event->angleDelta().y()
                                                   : event->angleDelta().x();
    if (delta != 0) {
        QScrollBar* bar = horizontalScrollBar();
        bar->setValue(bar->value() - delta);
        event->accept();
        return;
    }
    QListWidget::wheelEvent(event);
}

QIcon EditorFilmstrip::makeThumbnail(const QPixmap& image) const
{
    // Downscaled once and cached by the item; the full-resolution pixmap is
    // never touched again during scrolling. Icon sizes are logical, so on a
    // scaled display the thumbnail has to be rendered at the higher pixel
    // count and told its ratio, or it is upscaled and looks soft.
    const qreal ratio = devicePixelRatioF();
    QPixmap thumbnail =
      image.scaled(QSize(ThumbnailWidth, ThumbnailHeight) * ratio,
                   Qt::KeepAspectRatio,
                   Qt::SmoothTransformation);
    thumbnail.setDevicePixelRatio(ratio);
    return QIcon(thumbnail);
}

void EditorFilmstrip::renumber()
{
    for (int i = 0; i < count(); ++i) {
        item(i)->setText(QString::number(i + 1));
    }
}
