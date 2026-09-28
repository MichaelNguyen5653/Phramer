// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#pragma once

#include "capturebutton.h"
#include "tools/capturetool.h"

#include <QMap>
#include <QPointer>
#include <QVector>

class QLabel;

class QWidget;
class QPropertyAnimation;

class CaptureToolButton : public CaptureButton
{
    Q_OBJECT

public:
    explicit CaptureToolButton(const CaptureTool::Type,
                               QWidget* parent = nullptr);
    ~CaptureToolButton();

    static const QList<CaptureTool::Type>& getIterableButtonTypes();
    static int getPriorityByButton(CaptureTool::Type);

    QString name() const;
    QString description() const;
    QIcon icon() const;
    CaptureTool* tool() const;

    void setColor(const QColor& c);
    void animatedShow();

    // Adds the tool's name under the button when the setting is on. Only the
    // capture overlay asks: the settings page shows these buttons as colour
    // previews, where a name would be noise.
    void enableNameLabel();
    // Size of the name label, or empty when there is none. ButtonHandler
    // spaces the buttons to fit it.
    QSize labelSize() const;
    // Where the name label sits, or a null rectangle when it is hidden or
    // absent. Painted hints must avoid it as well as the button.
    QRect labelGeometry() const;

protected:
    void mousePressEvent(QMouseEvent* e) override;
    void moveEvent(QMoveEvent* e) override;
    void showEvent(QShowEvent* e) override;
    void hideEvent(QHideEvent* e) override;
    static QList<CaptureTool::Type> iterableButtonTypes;

    CaptureTool* m_tool;

signals:
    void pressedButtonLeftClick(CaptureToolButton*);
    void pressedButtonRightClick(CaptureToolButton*);

private:
    CaptureToolButton(QWidget* parent = nullptr);
    CaptureTool::Type m_buttonType;

    QPropertyAnimation* m_emergeAnimation;
    // A sibling rather than a child: the button is masked to a circle, which
    // would clip anything drawn below it
    QPointer<QLabel> m_label;

    void placeLabel();

    static QColor m_mainColor;

    void initButton();
    void updateIcon();
    // Pops the tool's variant picker, for tools that have one
    void showOptionsMenu();
};
