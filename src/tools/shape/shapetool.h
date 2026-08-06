// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/abstracttwopointtool.h"

#include <QPainterPath>
#include <functional>

class QMenu;

/**
 * @brief One toolbar button covering every closed shape annotation.
 *
 * Consolidates what used to be three separate buttons — Rectangle, Circle
 * and "Rectangular Selection" — into a shape choice crossed with a style
 * choice, picked from a menu on the button. Those three tools still exist
 * and keep their shortcuts, since their type numbers are persisted in user
 * configurations and cannot be removed; they are simply no longer part of
 * the default toolbar.
 *
 * The chosen variant lives in the configuration rather than on the tool, so
 * the toolbar button and the editor's copy of the picker always agree. A
 * placed object captures the variant it was drawn with through copyParams,
 * so changing the picker afterwards never rewrites existing annotations.
 */
class ShapeTool : public AbstractTwoPointTool
{
    Q_OBJECT
public:
    enum class Kind
    {
        Square = 0,
        Circle = 1,
    };

    enum class Style
    {
        Hollow = 0,
        Filled = 1,
        Highlight = 2,
    };

    explicit ShapeTool(QObject* parent = nullptr);

    QIcon icon(const QColor& background, bool inEditor) const override;
    QString name() const override;
    QString description() const override;

    bool hasOptionsMenu() const override;
    QMenu* optionsMenu(QWidget* parent) override;

    CaptureTool* copy(QObject* parent = nullptr) override;
    void process(QPainter& painter, const QPixmap& pixmap) override;

    // Icon file name for a variant, so the toolbar button and the editor's
    // menu can label themselves without constructing a tool
    static QString iconName(Kind kind, Style style);
    static Kind configuredKind();
    static Style configuredStyle();

    /**
     * @brief Builds the shape/style picker.
     *
     * Shared by the capture overlay's round tool button and the editor's
     * toolbar so the two cannot drift apart. onChanged fires after the
     * configuration has been written, letting the caller refresh its icon.
     */
    static QMenu* buildMenu(QWidget* parent,
                            const std::function<void()>& onChanged);

protected:
    void copyParams(const ShapeTool* from, ShapeTool* to);
    CaptureTool::Type type() const override;

public slots:
    void drawStart(const CaptureContext& context) override;
    void pressed(CaptureContext& context) override;

private:
    // The filled area, inflated by the tool size the same way the rectangle
    // tool does it, so filled and highlight cover exactly the same region
    QPainterPath filledPath() const;

    Kind m_kind;
    Style m_style;
    // Highlight border width, snapshotted at drawStart. See HighlightStyle.
    int m_outlineWidth;
};
