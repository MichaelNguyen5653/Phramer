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

protected:
    // QListWidget maps the wheel onto the vertical bar, which does not exist
    // in a single-row strip
    void wheelEvent(QWheelEvent* event) override;

private:
    QIcon makeThumbnail(const QPixmap& image) const;
    void renumber();

    bool m_updatingSelection{ false };
};
