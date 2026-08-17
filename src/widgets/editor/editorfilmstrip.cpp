// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/editor/editorfilmstrip.h"

#include <QDragMoveEvent>
#include <QDropEvent>
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

    // Reordering the strip reorders the session, so the drag has to be an
    // internal move: dropping a tile anywhere else must not hand the image
    // out or make a copy of it.
    //
    // This has to stay below setMovement above. Movement is not just a layout
    // setting — setMovement(Static) calls setDragEnabled(false) and clears
    // acceptDrops on the viewport, so anything enabled before it is silently
    // switched back off. Static itself is still what is wanted: it keeps icon
    // mode laying the tiles out as one row instead of letting a dropped tile
    // stay wherever it was released.
    setDragEnabled(true);
    setAcceptDrops(true);
    viewport()->setAcceptDrops(true);
    setDropIndicatorShown(true);
    setDragDropMode(QAbstractItemView::InternalMove);
    setDefaultDropAction(Qt::MoveAction);

    // Touch and trackpad flick. Left-button dragging is deliberately not
    // grabbed, since that would swallow click-to-select and, now, the
    // reorder drag.
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

void EditorFilmstrip::startDrag(Qt::DropActions supportedActions)
{
    // Pressing a tile has already made it current, so this is the tile under
    // the cursor. Recorded here because the drop needs the source row and the
    // selection may have moved on by then.
    m_dragRow = currentRow();
    QListWidget::startDrag(supportedActions);
}

void EditorFilmstrip::dragMoveEvent(QDragMoveEvent* event)
{
    // Run the base class first so the drop indicator is drawn and the strip
    // auto-scrolls near its edges
    QListWidget::dragMoveEvent(event);

    // Icon mode refuses the drop unless it believes it can place the item
    // itself. This strip does the placing, so every point over the viewport
    // is a legal target — including past the last tile, which means "last".
    if (event->source() == this && m_dragRow >= 0) {
        event->setDropAction(Qt::MoveAction);
        event->accept();
    }
}

int EditorFilmstrip::dropRow(const QPoint& pos) const
{
    const QModelIndex index = indexAt(pos);
    if (!index.isValid()) {
        // Past the end of the row, or in the gap under it
        return count();
    }
    // Past the middle of a tile means after it, which is what makes dropping
    // onto the right half of the last tile reach the end of the strip
    const QRect rect = visualRect(index);
    return pos.x() > rect.center().x() ? index.row() + 1 : index.row();
}

void EditorFilmstrip::dropEvent(QDropEvent* event)
{
    if (event->source() != this || m_dragRow < 0 || m_dragRow >= count()) {
        QListWidget::dropEvent(event);
        return;
    }

    const int from = m_dragRow;
    m_dragRow = -1;

    const int insertAt = dropRow(event->position().toPoint());
    // dropRow indexes the list with the dragged tile still in it; taking that
    // tile out shifts everything after it down one place
    const int to = insertAt > from ? insertAt - 1 : insertAt;

    // Both no-op drops: onto itself, or into the gap it already occupies
    if (to == from) {
        event->ignore();
        return;
    }

    // Guarded across the whole move, not just the final setCurrentRow. Taking
    // a row out moves the current row on its own, and that reaches the window
    // as a navigation request against a session it has not reordered yet —
    // which commits the active tool onto whichever image the stale index
    // happens to name. The window is told once, through imagesReordered.
    m_updatingSelection = true;
    QListWidgetItem* tile = takeItem(from);
    insertItem(to, tile);
    setCurrentRow(to);
    m_updatingSelection = false;

    renumber();
    scrollToItem(tile, QAbstractItemView::EnsureVisible);

    // Reported as ignored even though the drop was handled. Icon mode ends a
    // drag with "if the action was MoveAction and I did not move the row
    // myself, delete the row" — and the flag it checks is private to
    // QAbstractItemView::dropEvent, which this override replaces. Answering
    // MoveAction would reorder the tile and then delete it.
    event->setDropAction(Qt::IgnoreAction);
    event->accept();
    emit imagesReordered(from, to);
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
