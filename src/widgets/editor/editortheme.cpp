// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/editor/editortheme.h"

#include "utils/colorutils.h"

#include <QGuiApplication>
#include <QStyleHints>

namespace {

// Style sheets take rgba() with a 0-255 alpha; QColor::name() drops alpha
QString css(const QColor& c)
{
    return QStringLiteral("rgba(%1, %2, %3, %4)")
      .arg(c.red())
      .arg(c.green())
      .arg(c.blue())
      .arg(c.alpha());
}

} // namespace

EditorTheme EditorTheme::light()
{
    EditorTheme t;
    t.dark = false;
    t.window = QColor(0xF6, 0xF6, 0xF7);
    t.surface = QColor(0xFF, 0xFF, 0xFF);
    t.backdrop = QColor(0xE6, 0xE6, 0xE9);
    t.border = QColor(0xE1, 0xE1, 0xE6);
    t.text = QColor(0x1B, 0x1B, 0x1F);
    t.muted = QColor(0x6B, 0x6B, 0x76);
    t.hover = QColor(0, 0, 0, 13);
    t.pressed = QColor(0, 0, 0, 24);
    t.checked = QColor(0, 0, 0, 20);
    t.checkedBorder = QColor(0, 0, 0, 30);
    t.accent = QColor(0xFF, 0x63, 0x63);
    t.scrollHandle = QColor(0, 0, 0, 56);
    return t;
}

EditorTheme EditorTheme::darkTheme()
{
    EditorTheme t;
    t.dark = true;
    t.window = QColor(0x1C, 0x1C, 0x1F);
    t.surface = QColor(0x26, 0x26, 0x2A);
    t.backdrop = QColor(0x12, 0x12, 0x14);
    t.border = QColor(0x30, 0x30, 0x35);
    t.text = QColor(0xEC, 0xEC, 0xF1);
    t.muted = QColor(0x9B, 0x9B, 0xA4);
    t.hover = QColor(255, 255, 255, 18);
    t.pressed = QColor(255, 255, 255, 28);
    t.checked = QColor(255, 255, 255, 30);
    t.checkedBorder = QColor(255, 255, 255, 46);
    t.accent = QColor(0xFF, 0x63, 0x63);
    t.scrollHandle = QColor(255, 255, 255, 46);
    return t;
}

EditorTheme EditorTheme::current()
{
    switch (QGuiApplication::styleHints()->colorScheme()) {
        case Qt::ColorScheme::Dark:
            return darkTheme();
        case Qt::ColorScheme::Light:
            return light();
        default:
            // No opinion from the platform: go by what the system palette
            // already is
            return ColorUtils::colorIsDark(
                     QGuiApplication::palette().color(QPalette::Window))
                     ? darkTheme()
                     : light();
    }
}

QPalette EditorTheme::palette(QPalette base) const
{
    base.setColor(QPalette::Window, window);
    base.setColor(QPalette::WindowText, text);
    base.setColor(QPalette::Base, surface);
    base.setColor(QPalette::AlternateBase, window);
    base.setColor(QPalette::Text, text);
    base.setColor(QPalette::Button, surface);
    base.setColor(QPalette::ButtonText, text);
    base.setColor(QPalette::Highlight, accent);
    base.setColor(QPalette::HighlightedText, Qt::white);
    base.setColor(QPalette::ToolTipBase, surface);
    base.setColor(QPalette::ToolTipText, text);
    base.setColor(QPalette::PlaceholderText, muted);
    // The canvas scroll area paints its empty space with Dark
    base.setColor(QPalette::Dark, backdrop);
    base.setColor(QPalette::Mid, border);
    base.setColor(QPalette::Disabled, QPalette::WindowText, muted);
    base.setColor(QPalette::Disabled, QPalette::Text, muted);
    base.setColor(QPalette::Disabled, QPalette::ButtonText, muted);
    return base;
}

QString EditorTheme::styleSheet() const
{
    // Scoped by object name so dialogs parented to the editor, such as the
    // colour picker, keep the platform look
    QString sheet = QStringLiteral(R"(
QMainWindow#phramerEditor { background: @window; }

QToolBar#editorToolBar {
    background: @window;
    border: none;
    border-bottom: 1px solid @border;
    padding: 6px 10px;
    spacing: 2px;
}
QToolBar#editorToolBar::separator {
    background: @border;
    width: 1px;
    margin: 10px 6px;
}
QToolBar#editorToolBar QToolButton {
    background: transparent;
    color: @text;
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 4px 6px;
    min-width: 38px;
}
QToolBar#editorToolBar QToolButton:hover { background: @hover; }
QToolBar#editorToolBar QToolButton:pressed { background: @pressed; }
QToolBar#editorToolBar QToolButton:checked {
    background: @checked;
    border-color: @checkedBorder;
}
QToolBar#editorToolBar QToolButton:disabled { color: @muted; }
QToolBar#editorToolBar QToolButton[popupMode="1"] { padding-right: 16px; }
QToolBar#editorToolBar QToolButton::menu-button {
    border: none;
    border-top-right-radius: 8px;
    border-bottom-right-radius: 8px;
    width: 14px;
}
QToolBar#editorToolBar QToolButton::menu-button:hover { background: @hover; }
QToolBar#editorToolBar QLabel { color: @muted; }
QToolBar#editorToolBar QSpinBox {
    background: @surface;
    color: @text;
    border: 1px solid @border;
    border-radius: 7px;
    padding: 3px 6px;
    min-width: 46px;
}
QToolBar#editorToolBar QSpinBox:focus { border-color: @accent; }

QScrollArea#editorPage { border: none; }

QListWidget#editorFilmstrip {
    background: @window;
    border: none;
    border-top: 1px solid @border;
    outline: none;
    color: @muted;
}
QListWidget#editorFilmstrip::item {
    border: 1px solid transparent;
    border-radius: 10px;
    padding: 4px;
}
QListWidget#editorFilmstrip::item:hover { background: @hover; }
QListWidget#editorFilmstrip::item:selected {
    background: @checked;
    border-color: @checkedBorder;
    color: @text;
}

QStatusBar#editorStatus {
    background: @window;
    color: @muted;
    border-top: 1px solid @border;
}
QStatusBar#editorStatus QLabel { color: @muted; }
QStatusBar#editorStatus QPushButton {
    background: transparent;
    color: @text;
    border: 1px solid @border;
    border-radius: 7px;
    padding: 3px 12px;
}
QStatusBar#editorStatus QPushButton:hover { background: @hover; }
QStatusBar#editorStatus QPushButton:pressed { background: @pressed; }
QStatusBar#editorStatus QPushButton:disabled {
    color: @muted;
    border-color: transparent;
}

QMainWindow#phramerEditor QMenu {
    background: @surface;
    color: @text;
    border: 1px solid @border;
    padding: 4px;
}
QMainWindow#phramerEditor QMenu::item { padding: 6px 16px; border-radius: 6px; }
QMainWindow#phramerEditor QMenu::item:selected { background: @hover; }

QMainWindow#phramerEditor QToolTip {
    background: @surface;
    color: @text;
    border: 1px solid @border;
    padding: 4px 8px;
}

QMainWindow#phramerEditor QScrollBar:horizontal {
    background: transparent; height: 10px; margin: 2px;
}
QMainWindow#phramerEditor QScrollBar:vertical {
    background: transparent; width: 10px; margin: 2px;
}
QMainWindow#phramerEditor QScrollBar::handle {
    background: @scroll; border-radius: 3px; min-width: 32px; min-height: 32px;
}
QMainWindow#phramerEditor QScrollBar::add-line,
QMainWindow#phramerEditor QScrollBar::sub-line { width: 0px; height: 0px; }
QMainWindow#phramerEditor QScrollBar::add-page,
QMainWindow#phramerEditor QScrollBar::sub-page { background: transparent; }
)");
    // Longest tokens first, so "@checkedBorder" is not eaten by "@checked"
    sheet.replace(QStringLiteral("@checkedBorder"), css(checkedBorder));
    sheet.replace(QStringLiteral("@checked"), css(checked));
    sheet.replace(QStringLiteral("@window"), css(window));
    sheet.replace(QStringLiteral("@surface"), css(surface));
    sheet.replace(QStringLiteral("@border"), css(border));
    sheet.replace(QStringLiteral("@text"), css(text));
    sheet.replace(QStringLiteral("@muted"), css(muted));
    sheet.replace(QStringLiteral("@hover"), css(hover));
    sheet.replace(QStringLiteral("@pressed"), css(pressed));
    sheet.replace(QStringLiteral("@accent"), css(accent));
    sheet.replace(QStringLiteral("@scroll"), css(scrollHandle));
    return sheet;
}
