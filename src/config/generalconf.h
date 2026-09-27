// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#pragma once

#include <QScrollArea>
#include <QVector>
#include <QWidget>

class QVBoxLayout;
class QCheckBox;
class QLayout;
class QPushButton;
class QLabel;
class QLineEdit;
class QSpinBox;
class QComboBox;

class GeneralConf : public QWidget
{
    Q_OBJECT
public:
    // The same class builds both tabs so every setting keeps one
    // implementation; the page only decides which rows are created
    enum class Page
    {
        General,
        Advanced
    };
    explicit GeneralConf(Page page, QWidget* parent = nullptr);
    enum xywh_position
    {
        xywh_none = 0,
        xywh_top_left = 1,
        xywh_bottom_left = 2,
        xywh_top_right = 3,
        xywh_bottom_right = 4,
        xywh_center = 5
    };

public slots:
    void updateComponents();

protected:
    void changeEvent(QEvent* event) override;

private slots:
    void showHelpChanged(bool checked);
    void saveLastRegion(bool checked);
    void showSidePanelButtonChanged(bool checked);
    void showDesktopNotificationChanged(bool checked);
    void showAbortNotificationChanged(bool checked);
#if !defined(DISABLE_UPDATE_CHECKER)
    void checkForUpdatesChanged(bool checked);
#endif
    void allowMultipleGuiInstancesChanged(bool checked);
    void autoCloseIdleDaemonChanged(bool checked);
    void autostartChanged(bool checked);
    void historyConfirmationToDelete(bool checked);
    void uploadHistoryMaxChanged(int max);
    void undoLimit(int limit);
    void saveAfterCopyChanged(bool checked);
    void changeSavePath();
    void importConfiguration();
    void exportFileConfiguration();
    void resetConfiguration();
    void togglePathFixed();
    void uploadClientKeyEdited();
    void useJpgForClipboardChanged(bool checked);
    void setSaveAsFileExtension(const QString& extension);
    void setGeometryLocation(int index);
    void setSelGeoHideTime(int v);
    void setJpegQuality(int v);
    void setReverseArrow(bool checked);
    void setInsecurePixelate(bool checked);
    void applySearchFilter(const QString& query);
#if !defined(Q_OS_MACOS)
    void captureRegionModeChanged(int index);
#endif
#if defined(Q_OS_MACOS)
    void useNativeFullscreenChanged(bool checked);
#endif
#if defined(Q_OS_UNIX) && !defined(Q_OS_MACOS)
    void useX11LegacyScreenshotChanged(bool checked);
#endif

private:
    const QString chooseFolder(const QString& currentPath = "");

    void initAllowMultipleGuiInstances();
    void initAntialiasingPinZoom();
    void initAutoOpenInEditor();
    void initSearchBox();
    // Snapshot of every filterable row, taken once the page is fully built
    void buildSearchIndex();
    void initAutoCloseIdleDaemon();
    void initAutostart();
#if !defined(DISABLE_UPDATE_CHECKER)
    void initCheckForUpdates();
#endif
    void initConfigButtons();
    void initCopyAndCloseAfterUpload();
    void initCopyOnDoubleClick();
    void initCopyPathAfterSave();
    void initHistoryConfirmationToDelete();
    void initPredefinedColorPaletteLarge();
    void initSaveAfterCopy();
    void initScrollArea();
    void initShowDesktopNotification();
    void initShowAbortNotification();
    void initShowHelp();
    void initShowMagnifier();
    void initShowEditorHint();
    void initShowToolLabels();
    void initShowQuitPrompt();
    void initShowSidePanelButton();
    void initShowStartupLaunchMessage();
    void initShowTrayIcon();
    void initSquareMagnifier();
    void initUndoLimit();
    void initUploadWithoutConfirmation();
    void initUseJpgForClipboard();
    void initUploadHistoryMax();
    void initUploadClientSecret();
    void initSaveLastRegion();
    void initShowSelectionGeometry();
    void initJpegQuality();
    void initReverseArrow();
    void initInsecurePixelate();
#if !defined(Q_OS_MACOS)
    void initCaptureRegionMode();
#endif
#if defined(Q_OS_WIN)
    void initShowWelcomeMessage();
    void initScreenClipProtocol();
    // Status text and button for whatever the registry says right now
    void updateScreenClipRow();
    void toggleScreenClipRegistration();
#endif
#if defined(Q_OS_MACOS)
    void initUseNativeFullscreen();
#endif
#if defined(Q_OS_UNIX) && !defined(Q_OS_MACOS)
    void initUseX11LegacyScreenshot();
#endif

    void _updateComponents(bool allowEmptySavePath);

    // One filterable entry: either a widget on the page or a row of widgets
    // sharing a layout. Only one of the two is ever set.
    struct SearchRow
    {
        QString text;
        QWidget* widget;
        QLayout* layout;
    };

    // class members
    Page m_page;
    QVBoxLayout* m_layout{ nullptr };
    QLineEdit* m_searchBox{ nullptr };
    QVector<SearchRow> m_searchRows;
    QVBoxLayout* m_scrollAreaLayout{ nullptr };
    QScrollArea* m_scrollArea{ nullptr };
    QCheckBox* m_sysNotifications{ nullptr };
    QCheckBox* m_abortNotifications{ nullptr };
    QCheckBox* m_showTray{ nullptr };
    QCheckBox* m_helpMessage{ nullptr };
    QCheckBox* m_sidePanelButton{ nullptr };
#if !defined(DISABLE_UPDATE_CHECKER)
    QCheckBox* m_checkForUpdates{ nullptr };
#endif
    QCheckBox* m_allowMultipleGuiInstances{ nullptr };
    QCheckBox* m_autoOpenInEditor{ nullptr };
    QCheckBox* m_autoCloseIdleDaemon{ nullptr };
    QCheckBox* m_autostart{ nullptr };
    QCheckBox* m_showStartupLaunchMessage{ nullptr };
    QCheckBox* m_showQuitPrompt{ nullptr };
    QCheckBox* m_copyURLAfterUpload{ nullptr };
    QCheckBox* m_copyPathAfterSave{ nullptr };
    QCheckBox* m_antialiasingPinZoom{ nullptr };
    QCheckBox* m_saveLastRegion{ nullptr };
    QCheckBox* m_uploadWithoutConfirmation{ nullptr };
    QPushButton* m_importButton{ nullptr };
    QPushButton* m_exportButton{ nullptr };
    QPushButton* m_resetButton{ nullptr };
    QCheckBox* m_saveAfterCopy{ nullptr };
    QLineEdit* m_savePath{ nullptr };
    QLineEdit* m_uploadClientKey{ nullptr };
    QPushButton* m_changeSaveButton{ nullptr };
    QCheckBox* m_screenshotPathFixedCheck{ nullptr };
    QCheckBox* m_historyConfirmationToDelete{ nullptr };
    QCheckBox* m_useJpgForClipboard{ nullptr };
    QSpinBox* m_uploadHistoryMax{ nullptr };
    QSpinBox* m_undoLimit{ nullptr };
    QComboBox* m_setSaveAsFileExtension{ nullptr };
    QCheckBox* m_predefinedColorPaletteLarge{ nullptr };
    QCheckBox* m_showMagnifier{ nullptr };
    QCheckBox* m_showEditorHint{ nullptr };
    QCheckBox* m_showToolLabels{ nullptr };
    QCheckBox* m_squareMagnifier{ nullptr };
    QCheckBox* m_copyOnDoubleClick{ nullptr };
    QCheckBox* m_showSelectionGeometry{ nullptr };
    QComboBox* m_selectGeometryLocation{ nullptr };
    QSpinBox* m_xywhTimeout{ nullptr };
    QSpinBox* m_jpegQuality{ nullptr };
    QCheckBox* m_reverseArrow{ nullptr };
    QCheckBox* m_insecurePixelate{ nullptr };
#if !defined(Q_OS_MACOS)
    QComboBox* m_captureRegionMode{ nullptr };
#endif
#if defined(Q_OS_WIN)
    QCheckBox* m_showWelcomeMessage{ nullptr };
    QLabel* m_screenClipStatus{ nullptr };
    QPushButton* m_screenClipButton{ nullptr };
    QPushButton* m_screenClipSettingsButton{ nullptr };
#endif
#if defined(Q_OS_MACOS)
    QCheckBox* m_useNativeFullscreen{ nullptr };
#endif
#if defined(Q_OS_UNIX) && !defined(Q_OS_MACOS)
    QCheckBox* m_useX11LegacyScreenshot{ nullptr };
#endif
};
