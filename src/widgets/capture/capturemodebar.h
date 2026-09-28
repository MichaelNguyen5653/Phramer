// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QWidget>

class QPushButton;

/**
 * @brief The capture overlay's photo/video switch.
 *
 * Shown at the top of the overlay until the user starts a selection. Photo
 * is what the overlay already does, so choosing it changes nothing. Video
 * hands off to Snipping Tool (see utils/snippingtool.h) when the user has
 * opted in; otherwise the button stays visible but greyed, and says where
 * to turn it on rather than silently doing nothing.
 *
 * The keys shown on the buttons are read from the configured shortcuts.
 * The bar never handles keys itself: the overlay owns the keyboard.
 */
class CaptureModeBar : public QWidget
{
    Q_OBJECT
public:
    explicit CaptureModeBar(QWidget* parent = nullptr);

    // Whether choosing Video would do anything right now
    bool videoAvailable() const { return m_videoAvailable; }

    // Explains why Video is unavailable, next to its button
    void explainVideoUnavailable();

    // Centres the bar at the top of `area`, in parent coordinates
    void placeIn(const QRect& area);

signals:
    void videoRequested();

private:
    QPushButton* m_photoButton{ nullptr };
    QPushButton* m_videoButton{ nullptr };
    QString m_unavailableReason;
    bool m_videoAvailable{ false };
};
