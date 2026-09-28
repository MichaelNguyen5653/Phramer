// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QDialog>
#include <QVector>

class QCheckBox;
class QLabel;
class QParallelAnimationGroup;
class QPushButton;
class PrintScreenZoom;
class QWidget;
class WelcomeLogo;

/**
 * @brief The welcome shown once per version: three pages on a frameless,
 * rounded card that follows the Windows light/dark setting.
 *
 * Pages are not slid as whole widgets. Each page lists the widgets that
 * "reveal" it, and entering a page fades and lifts those in one after
 * another. Only leaf widgets ever carry a graphics effect, because Qt does
 * not composite an effect nested inside another one.
 */
class WelcomeTour : public QDialog
{
    Q_OBJECT
public:
    explicit WelcomeTour(QWidget* parent = nullptr);

    /**
     * @brief Shows the welcome if this version has not shown it yet.
     *
     * "Don't show again" from older tours silences release notes within a
     * major version; a new major version is shown once regardless, and is
     * mandatory: it cannot be skipped, and counts as seen only once its last
     * page is reached. Any other release records the version before the
     * dialog is answered, so closing it any way at all counts as seen.
     * Returns whether it was shown.
     */
    static bool showIfDue(QWidget* parent = nullptr);

    // Shows it unconditionally. Not mandatory for the tray's "What's New".
    static void showNow(QWidget* parent = nullptr, bool mandatory = false);

public slots:
    // Refused while a mandatory tour has not reached its last page; Escape
    // and Alt+F4 both arrive here
    void reject() override;

protected:
    void paintEvent(QPaintEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void showEvent(QShowEvent* event) override;

private:
    struct Page
    {
        QWidget* widget = nullptr;
        // Revealed in this order when the page is entered
        QVector<QWidget*> reveal;
    };

    void buildWelcomePage();
    void buildScreenClipPage();
    void buildVideoPage();
    void buildFeaturesPage();
    void buildFixesPage();
    void buildFooter();
    QWidget* makeTitle(const QString& text, QWidget* parent, qreal scale);
    QWidget* makeRow(const QString& icon,
                     const QString& title,
                     const QString& body,
                     QWidget* parent);

    void goTo(int index);
    void reveal(const Page& page);
    void updateFooter();
    void advance();
    // Runs the ms-screenclip registration if it was asked for. Returns
    // whether the tour may move past the MS-SCREENCLIP page.
    bool registerScreenClipIfChecked();
    // Tells the user to pick Phramer in Windows Settings, with a link there
    QString screenClipFinishHint() const;
    void openEditor();
    void openSettings();

    QWidget* m_card{ nullptr };
    QWidget* m_content{ nullptr };
    QVector<Page> m_pages;
    WelcomeLogo* m_logo{ nullptr };
    QVector<QLabel*> m_pips;
    // Windows only; null elsewhere
    QWidget* m_screenClipPage{ nullptr };
    PrintScreenZoom* m_zoom{ nullptr };
    QCheckBox* m_screenClipBox{ nullptr };
    QLabel* m_screenClipStatus{ nullptr };

    QPushButton* m_skipButton{ nullptr };
    QPushButton* m_nextButton{ nullptr };
    QPushButton* m_settingsButton{ nullptr };
    QPushButton* m_editorButton{ nullptr };

    QParallelAnimationGroup* m_revealAnimation{ nullptr };
    int m_index{ -1 };
    bool m_dark{ false };
    bool m_mandatory{ false };
    bool m_reachedEnd{ false };
    // Card-drag state; a null point means no drag is in progress
    QPoint m_dragOffset;
};
