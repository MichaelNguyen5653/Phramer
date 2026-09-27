#include <QSystemTrayIcon>

#pragma once

class QAction;

class TrayIcon : public QSystemTrayIcon
{
    Q_OBJECT
public:
    TrayIcon(QObject* parent = nullptr);
    virtual ~TrayIcon();

#if !defined(DISABLE_UPDATE_CHECKER)
    QAction* appUpdates();
#if defined(Q_OS_WIN)
    // Amber dot on the tray icon while an update is available
    void showUpdateBadge(const QString& version);
    void clearUpdateBadge();
#endif
#endif

private:
    void initTrayIcon();
    void initMenu();
    void initScreenMenu();
    void updateCaptureActionShortcut();
#if !defined(DISABLE_UPDATE_CHECKER)
    void updateCheckUpdatesMenuVisibility();
#endif

    void startGuiCapture();
    void startGuiCaptureOnScreen(int screenIndex);

    QMenu* m_menu;
    QMenu* m_screenMenu;
#if defined(Q_OS_WIN)
    QIcon m_baseIcon;
#endif
    QAction* m_captureAction;
    QAction* m_launcherAction;
    QAction* m_whatsNewAction{ nullptr };
    QAction* m_infoAction;
#if !defined(DISABLE_UPDATE_CHECKER)
    QAction* m_appUpdates;
#endif
};
