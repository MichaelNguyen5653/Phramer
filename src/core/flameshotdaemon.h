#pragma once

#include <QByteArray>
#include <QObject>

#if !(defined(Q_OS_MACOS) || defined(Q_OS_WIN))
#include <QtDBus/QDBusAbstractAdaptor>
#endif

class QPixmap;
class QRect;
class QDBusMessage;
class QDBusConnection;
class TrayIcon;
class CaptureWidget;

#if !defined(DISABLE_UPDATE_CHECKER)
class QNetworkAccessManager;
class QNetworkReply;
class QVersionNumber;
#endif

class FlameshotDaemon : public QObject
{
    Q_OBJECT
public:
    static void start();
    static FlameshotDaemon* instance();
    static void createPin(const QPixmap& capture, QRect geometry);
    // Asks the running daemon to start a capture, exactly as its own hotkey
    // would. Returns false when there is no daemon to ask.
    static bool requestGui();
    static void copyToClipboard(const QPixmap& capture);
    static void copyToClipboard(const QString& text,
                                const QString& notification = "");
    // Puts a saved file (not its pixels) on the clipboard; see
    // utils/filehandoff.h
    static void copyFileToClipboard(const QString& path);
    static bool isThisInstanceHostingWidgets();

    // `savedFile`, when given, is what clicking the notification shows in
    // Explorer
    void sendTrayNotification(
      const QString& text,
      const QString& title = QStringLiteral("Phramer Info"),
      const int timeout = 5000,
      const QString& savedFile = QString());

#if defined(USE_KDSINGLEAPPLICATION) &&                                        \
  (defined(Q_OS_MACOS) || defined(Q_OS_WIN))
public slots:
    void messageReceivedFromSecondaryInstance(const QByteArray& message);
#endif

#if !defined(DISABLE_UPDATE_CHECKER)
public:
    void showUpdateNotificationIfAvailable(CaptureWidget* widget);

public slots:
    void checkForUpdates();
    void getLatestAvailableVersion();

private slots:
    void handleReplyCheckUpdates(QNetworkReply* reply);

signals:
    void newVersionAvailable(QVersionNumber version);

#if defined(Q_OS_WIN)
public slots:
    // Download the new installer, verify it against its published
    // checksum, run it and restart the application. Falls back to opening
    // the release page when the release carries no verifiable installer.
    void startUpdateAndRestart();

private:
    void downloadUpdateInstaller();
    void applyUpdate(const QString& msiPath);
    void failUpdate(const QString& reason);

    // Direct asset URLs from the latest release; empty when the release
    // has no installer / checksum, in which case only the browser
    // fallback is offered
    QString m_appLatestMsiUrl;
    QString m_appLatestShaUrl;
    QString m_expectedMsiSha256;
    bool m_updateInProgress = false;

    // Downloads must not share m_networkCheckUpdates: that manager's
    // finished signal is wired to handleReplyCheckUpdates, which consumes
    // the reply body as JSON, so a shared manager empties the buffer
    // before the download handler can read it
    QNetworkAccessManager* m_updateDownloader = nullptr;
#endif
#endif

#if defined(Q_OS_WIN)
private slots:
    // Greets the user on the first launch after installation and offers to
    // free up the Print Screen key. Disables itself afterwards.
    void showWelcomeMessage();
#endif

private:
    FlameshotDaemon();
    void quitIfIdle();
    void attachPin(const QPixmap& pixmap, QRect geometry);
    void attachScreenshotToClipboard(const QPixmap& pixmap);

    void attachPin(const QByteArray& data);
    void attachScreenshotToClipboard(const QByteArray& screenshot);
    void attachTextToClipboard(const QString& text,
                               const QString& notification);
    void attachFileToClipboard(const QString& path);

    void initTrayIcon();
    void enableTrayIcon(bool enable);

#if !(defined(Q_OS_MACOS) || defined(Q_OS_WIN))
    static QDBusMessage createMethodCall(const QString& method);
    static void checkDBusConnection(const QDBusConnection& connection);
    static void call(const QDBusMessage& m);
#endif

    bool m_persist;
    bool m_hostingClipboard;
    bool m_clipboardSignalBlocked;
    QList<QWidget*> m_widgets;
    TrayIcon* m_trayIcon;

#if !defined(DISABLE_UPDATE_CHECKER)
    // The file behind the latest tray notification, if it had one: the
    // tray reports a click, not which message was clicked
    QString m_notificationFile;
    QString m_appLatestUrl;
    QString m_appLatestVersion;
    bool m_showManualCheckAppUpdateStatus;
    QNetworkAccessManager* m_networkCheckUpdates;
#endif

    static FlameshotDaemon* m_instance;

#if !(defined(Q_OS_MACOS) || defined(Q_OS_WIN))
    friend class FlameshotDBusAdapter;
#endif
};
