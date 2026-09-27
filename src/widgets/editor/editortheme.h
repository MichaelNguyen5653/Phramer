// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QColor>
#include <QPalette>
#include <QString>

/**
 * @brief The editor window's look: a small set of colour tokens, in a light
 * and a dark variant, rendered into one style sheet and one palette.
 *
 * The palette matters as much as the style sheet. The tool icons come in a
 * black and a white set and are picked by the brightness of
 * QPalette::Window, and the canvas backdrop is QPalette::Dark, so both follow
 * the theme without knowing it exists.
 */
struct EditorTheme
{
    bool dark = false;
    QColor window;
    QColor surface;
    QColor backdrop;
    QColor border;
    QColor text;
    QColor muted;
    QColor hover;
    QColor pressed;
    QColor checked;
    QColor checkedBorder;
    QColor accent;
    QColor scrollHandle;

    // Follows the Windows light/dark setting
    static EditorTheme current();
    static EditorTheme light();
    static EditorTheme darkTheme();

    QString styleSheet() const;
    QPalette palette(QPalette base) const;
};
