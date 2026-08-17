// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QListWidget>

/**
 * @brief Horizontal thumbnail strip along the bottom of the editor.
 *
 * Built on QListWidget in icon mode because the item view already culls items
 * outside the viewport and keeps one cached icon per row. Thumbnails are
 * generated once per image and refreshed on a coalescing timer, so drawing on
 * a 4K capture never rescales the full image per frame.
 */
class EditorFilmstrip : public QListWidget
{
    Q_OBJECT
public:
    explicit EditorFilmstrip(QWidget* parent = nullptr);

    void appendImage(const QPixmap& image);
    void updateImage(int index, const QPixmap& image);
    void removeImage(int index);
    void setActiveIndex(int index);

signals:
    void imageActivated(int index);
    // A tile was dragged to a new position. The strip has already reordered
    // itself; the listener has to bring its own model into line.
    void imagesReordered(int from, int to);

protected:
    // QListWidget maps the wheel onto the vertical bar, which does not exist
    // in a single-row strip
    void wheelEvent(QWheelEvent* event) override;

    void startDrag(Qt::DropActions supportedActions) override;
    void dragMoveEvent(QDragMoveEvent* event) override;
    // The reorder is done by hand rather than through the base class. Icon
    // mode only moves rows in the model when its layout is dynamic, and a
    // dynamic layout lets items take free positions, which scatters a strip
    // that has to stay a single ordered row.
    void dropEvent(QDropEvent* event) override;

private:
    QIcon makeThumbnail(const QPixmap& image) const;
    void renumber();
    // Row the drop would land on, in the list as it is before the dragged
    // tile is taken out of it
    int dropRow(const QPoint& pos) const;

    bool m_updatingSelection{ false };
    int m_dragRow{ -1 };
};
