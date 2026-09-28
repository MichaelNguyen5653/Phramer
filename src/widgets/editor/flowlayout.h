// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QLayout>
#include <QList>

/**
 * @brief A layout that wraps its items onto further rows when they do not fit.
 *
 * Holds the editor's annotation tools, which outgrew one toolbar row. It
 * reports heightForWidth, so the rows it needs are taken from the window's
 * height rather than hidden behind an overflow arrow. The packing itself is
 * FlowPacking::pack.
 */
class FlowLayout : public QLayout
{
public:
    explicit FlowLayout(QWidget* parent = nullptr, int spacing = 2);
    ~FlowLayout() override;

    void addItem(QLayoutItem* item) override;
    int count() const override;
    QLayoutItem* itemAt(int index) const override;
    QLayoutItem* takeAt(int index) override;

    Qt::Orientations expandingDirections() const override;
    bool hasHeightForWidth() const override;
    int heightForWidth(int width) const override;
    QSize minimumSize() const override;
    QSize sizeHint() const override;
    void setGeometry(const QRect& rect) override;

private:
    // Lays the items out in rect when apply is set; returns the height used
    int arrange(const QRect& rect, bool apply) const;

    QList<QLayoutItem*> m_items;
};
