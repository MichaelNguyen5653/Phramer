// SPDX-License-Identifier: GPL-3.0-or-later

#include "capturemodebar.h"

#include "utils/colorutils.h"
#include "utils/confighandler.h"
#if defined(Q_OS_WIN)
#include "utils/snippingtool.h"
#endif

#include <QCoreApplication>
#include <QHBoxLayout>
#include <QKeySequence>
#include <QPushButton>
#include <QToolTip>

namespace {

constexpr int TopMargin = 12;

QString withKey(const QString& label, const QString& shortcutName)
{
#if defined(Q_OS_WIN)
    const QString key = ConfigHandler().shortcut(shortcutName);
#else
    // The mode keys are only registered on Windows, and asking ConfigHandler
    // for an unregistered shortcut is treated as a configuration error
    Q_UNUSED(shortcutName)
    const QString key;
#endif
    if (key.isEmpty()) {
        return label;
    }
    return QStringLiteral("%1\n%2").arg(
      label,
      QCoreApplication::translate("CaptureModeBar", "(Press %1 to activate)")
        .arg(QKeySequence(key).toString(QKeySequence::NativeText)));
}

} // namespace

CaptureModeBar::CaptureModeBar(QWidget* parent)
  : QWidget(parent)
{
    setAttribute(Qt::WA_StyledBackground);
    setObjectName(QStringLiteral("captureModeBar"));

    auto* layout = new QHBoxLayout(this);
    layout->setContentsMargins(4, 4, 4, 4);
    layout->setSpacing(4);

    m_photoButton =
      new QPushButton(withKey(tr("Screenshot"), "CAPTURE_MODE_SCREENSHOT"));
    m_photoButton->setCheckable(true);
    m_photoButton->setChecked(true);
    // Photo is the only mode the overlay itself has, so it cannot be left
    m_photoButton->setEnabled(false);

    m_videoButton = new QPushButton(withKey(tr("Video"), "CAPTURE_MODE_VIDEO"));
    m_videoButton->setCursor(Qt::PointingHandCursor);

    for (QPushButton* button : { m_photoButton, m_videoButton }) {
        button->setFocusPolicy(Qt::NoFocus);
        layout->addWidget(button);
    }

#if defined(Q_OS_WIN)
    ConfigHandler config;
    if (!config.videoCaptureEnabled()) {
        m_unavailableReason =
          tr("Video capture is off. Turn it on in Settings, under Advanced.");
    } else if (!SnippingTool::isAvailable()) {
        m_unavailableReason = tr("Video capture needs the Windows Snipping "
                                 "Tool, which is not installed.");
    }
    m_videoAvailable = m_unavailableReason.isEmpty();
#else
    m_unavailableReason = tr("Video capture is only available on Windows.");
#endif
    m_videoButton->setToolTip(m_videoAvailable
                                ? tr("Record the screen with Snipping Tool")
                                : m_unavailableReason);
    // Kept enabled even when unavailable: a disabled button swallows the
    // click that should explain why it does nothing
    m_videoButton->setProperty("unavailable", !m_videoAvailable);

    connect(m_videoButton, &QPushButton::clicked, this, [this]() {
        if (m_videoAvailable) {
            emit videoRequested();
        } else {
            explainVideoUnavailable();
        }
    });

    const QColor accent = ConfigHandler().uiColor();
    const QColor accentText =
      ColorUtils::colorIsDark(accent) ? Qt::white : Qt::black;
    setStyleSheet(
      QStringLiteral(
        "#captureModeBar { background: rgba(30, 30, 30, 210);"
        "  border-radius: 10px; }"
        "QPushButton { color: white; background: transparent; border: none;"
        "  border-radius: 7px; padding: 6px 14px; }"
        "QPushButton:hover { background: rgba(255, 255, 255, 40); }"
        "QPushButton:checked, QPushButton:checked:disabled {"
        "  background: %1; color: %2; }"
        "QPushButton[unavailable=\"true\"] { color: rgba(255, 255, 255, 90); }"
        "QPushButton[unavailable=\"true\"]:hover {"
        "  background: rgba(255, 255, 255, 15); }")
        .arg(accent.name(), accentText.name()));
    adjustSize();
}

void CaptureModeBar::explainVideoUnavailable()
{
    QToolTip::showText(
      m_videoButton->mapToGlobal(QPoint(0, m_videoButton->height() + 4)),
      m_unavailableReason,
      m_videoButton);
}

void CaptureModeBar::placeIn(const QRect& area)
{
    adjustSize();
    move(area.x() + (area.width() - width()) / 2, area.y() + TopMargin);
    raise();
}
