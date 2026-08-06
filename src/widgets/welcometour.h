// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QDialog>
#include <QVector>

class QCheckBox;
class QLabel;
class QPropertyAnimation;
class QPushButton;
class QWidget;

/**
 * @brief The "Welcome to Phramer" tour, shown once per version.
 *
 * A horizontal carousel rather than a stack of dialogs: the slides live side
 * by side on one wide track inside a clipping viewport, and moving between
 * them animates the track's position. That is what makes the transition read
 * as a swipe instead of a page swap, and it lets a drag follow the pointer.
 */
class WelcomeTour : public QDialog
{
    Q_OBJECT
public:
    explicit WelcomeTour(QWidget* parent = nullptr);

    /**
     * @brief Shows the tour if this version has not shown it yet.
     *
     * Records the version before the dialog is answered, so dismissing it
     * with the window controls still counts as seen. Returns whether it was
     * shown.
     */
    static bool showIfDue(QWidget* parent = nullptr);

protected:
    void keyPressEvent(QKeyEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void done(int result) override;

private:
    void buildSlides();
    void buildChrome();
    QWidget* makeSlide(const QString& title,
                       const QString& body,
                       const QString& imageResource,
                       const QStringList& bullets);
    void goTo(int index, bool animate = true);
    void layoutTrack();
    void updateChrome();

    QWidget* m_viewport{ nullptr };
    QWidget* m_track{ nullptr };
    QVector<QWidget*> m_slides;
    QVector<QLabel*> m_dots;

    QPropertyAnimation* m_slideAnimation{ nullptr };
    QPushButton* m_backButton{ nullptr };
    QPushButton* m_nextButton{ nullptr };
    QPushButton* m_skipButton{ nullptr };
    QCheckBox* m_neverAgain{ nullptr };

    int m_index{ 0 };
    // Drag-to-swipe state; -1 means no drag is in progress
    int m_dragStartX{ -1 };
    int m_dragTrackX{ 0 };
};
