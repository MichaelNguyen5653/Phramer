#include "flameshotdaemon.h"
#include "core/flameshot.h"
#include "tools/pin/pinwidget.h"
#include "utils/abstractlogger.h"
#include "utils/confighandler.h"
#include "utils/globalvalues.h"
#include "utils/screenshotsaver.h"
#include "widgets/capture/capturewidget.h"
#include "widgets/trayicon.h"
#include "widgets/welcometour.h"

#include <QApplication>
#include <QClipboard>
#include <QIODevice>
#include <QPixmap>
#include <QRect>

#if !(defined(Q_OS_MACOS) || defined(Q_OS_WIN))
#include <QDBusConnection>
#include <QDBusMessage>
#endif

#if !defined(DISABLE_UPDATE_CHECKER)
#include <QDesktopServices>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QTimer>
#include <QUrl>
#endif

#if defined(USE_KDSINGLEAPPLICATION) &&                                        \
  (defined(Q_OS_MACOS) || defined(Q_OS_WIN))
#include <QBuffer>
#include <kdsingleapplication.h>
#endif

#ifdef Q_OS_WIN
#include "core/globalshortcutfilter.h"
#include "utils/printscreenkey.h"
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QIcon>
#include <QMessageBox>
#include <QProcess>
#include <QProgressDialog>
#include <QPushButton>
#include <QStandardPaths>
#include <QTextStream>
#include <QTimer>
#endif

/**
 * @brief A way of accessing the flameshot daemon both from the daemon itself,
 * and from subcommands.
 *
 * The daemon is necessary in order to:
 * - Host the system tray,
 * - Listen for hotkey events that will trigger captures,
 * - Host pinned screenshot widgets,
 * - Host the clipboard on X11, where the clipboard gets lost once flameshot
 *   quits.
 *
 * If the `autoCloseIdleDaemon` option is true, the daemon will close as soon as
 * it is not needed to host pinned screenshots and the clipboard.
 *
 * Both the daemon and non-daemon flameshot processes use the same public API,
 * which is implemented as static methods. In the daemon process, this class is
 * also instantiated as a singleton, so it can listen to D-Bus calls via the
 * sigslot mechanism. The instantiation is done by calling `start` (this must be
 * done only in the daemon process). Any instance (as opposed to static) members
 * can only be used if the current process is a daemon.
 *
 * @note The daemon will be automatically launched where necessary, via D-Bus.
 * This applies only to Linux.
 */
FlameshotDaemon::FlameshotDaemon()
  : m_persist(false)
  , m_hostingClipboard(false)
  , m_clipboardSignalBlocked(false)
  , m_trayIcon(nullptr)
#if !defined(DISABLE_UPDATE_CHECKER)
  , m_appLatestVersion(QStringLiteral(APP_VERSION).replace("v", ""))
  , m_showManualCheckAppUpdateStatus(false)
  , m_networkCheckUpdates(nullptr)
#endif
{
    connect(
      QApplication::clipboard(), &QClipboard::dataChanged, this, [this]() {
          if (!m_hostingClipboard || m_clipboardSignalBlocked) {
              m_clipboardSignalBlocked = false;
              return;
          }
          m_hostingClipboard = false;
          quitIfIdle();
      });

    m_persist = !ConfigHandler().autoCloseIdleDaemon();
    connect(ConfigHandler::getInstance(),
            &ConfigHandler::fileChanged,
            this,
            [this]() {
                ConfigHandler config;
                enableTrayIcon(!config.disabledTrayIcon());
                m_persist = !config.autoCloseIdleDaemon();
            });

#if !defined(DISABLE_UPDATE_CHECKER)
    if (ConfigHandler().checkForUpdates()) {
        getLatestAvailableVersion();
    }
#endif

    // Queued so the tray icon and the event loop are up before any dialog
    // appears. The tour comes first and is modal, so the Print Screen
    // question waits behind it rather than fighting it for focus.
    QTimer::singleShot(0, this, []() { WelcomeTour::showIfDue(); });

#if defined(Q_OS_WIN)
    if (ConfigHandler().showWelcomeMessage()) {
        QTimer::singleShot(0, this, &FlameshotDaemon::showWelcomeMessage);
    }
#endif
}

#if defined(Q_OS_WIN)
void FlameshotDaemon::showWelcomeMessage()
{
    // Flipped before the dialog is answered, so a user who dismisses it
    // with the window controls is not greeted again on every launch
    ConfigHandler().setShowWelcomeMessage(false);

    QMessageBox msgBox;
    msgBox.setWindowTitle(QStringLiteral("Phramer"));
    msgBox.setWindowIcon(GlobalValues::appIcon());
    msgBox.setIcon(QMessageBox::Question);
    msgBox.setText(QObject::tr("Welcome to Phramer!"));

    if (PrintScreenKey::isSnippingDisabled()) {
        msgBox.setIcon(QMessageBox::Information);
        msgBox.setInformativeText(
          QObject::tr("Press Print Screen, or the tray icon, to take a "
                      "screenshot."));
        msgBox.setStandardButtons(QMessageBox::Ok);
        msgBox.exec();
        return;
    }

    msgBox.setInformativeText(
      QObject::tr("Windows currently opens its own snipping tool when you "
                  "press Print Screen. Would you like Phramer to take over "
                  "that key instead?") +
      "\n\n" +
      QObject::tr("Phramer must be restarted for the change to take "
                  "effect."));
    QPushButton* yesBtn = msgBox.addButton(QMessageBox::Yes);
    msgBox.addButton(QMessageBox::No);
    msgBox.setDefaultButton(yesBtn);
    msgBox.exec();

    if (msgBox.clickedButton() == yesBtn &&
        !PrintScreenKey::disableSnipping()) {
        QMessageBox::warning(nullptr,
                             QStringLiteral("Phramer"),
                             QObject::tr("The registry could not be "
                                         "changed!"));
    }
}
#endif

void FlameshotDaemon::start()
{
    if (!m_instance) {
        m_instance = new FlameshotDaemon();
        // Tray icon needs FlameshotDaemon::instance() to be non-null
        m_instance->initTrayIcon();
        qApp->setQuitOnLastWindowClosed(false);
    }
}

void FlameshotDaemon::createPin(const QPixmap& capture, QRect geometry)
{
    if (instance()) {
        instance()->attachPin(capture, geometry);
        return;
    }

    QByteArray data;
    QDataStream stream(&data, QIODevice::WriteOnly);

#if defined(USE_KDSINGLEAPPLICATION) &&                                        \
  (defined(Q_OS_MACOS) || defined(Q_OS_WIN))
    auto kdsa = KDSingleApplication(QStringLiteral("com.phramer.Phramer"));
    stream << QStringLiteral("attachPin") << capture << geometry;
    kdsa.sendMessage(data);
#else
    stream << capture << geometry;
    QDBusMessage m = createMethodCall(QStringLiteral("attachPin"));
    m << data;
    call(m);
#endif
}

void FlameshotDaemon::copyToClipboard(const QPixmap& capture)
{
    if (instance()) {
        instance()->attachScreenshotToClipboard(capture);
        return;
    }

    QByteArray data;
    QDataStream stream(&data, QIODevice::WriteOnly);

#if defined(USE_KDSINGLEAPPLICATION) &&                                        \
  (defined(Q_OS_MACOS) || defined(Q_OS_WIN))
    auto kdsa = KDSingleApplication(QStringLiteral("com.phramer.Phramer"));
    stream << QStringLiteral("attachScreenshotToClipboard") << capture;
    kdsa.sendMessage(data);
#else
    stream << capture;
    QDBusMessage m =
      createMethodCall(QStringLiteral("attachScreenshotToClipboard"));

    m << data;
    call(m);
#endif
}

void FlameshotDaemon::copyToClipboard(const QString& text,
                                      const QString& notification)
{
    if (instance()) {
        instance()->attachTextToClipboard(text, notification);
        return;
    }

#if defined(USE_KDSINGLEAPPLICATION) &&                                        \
  (defined(Q_OS_MACOS) || defined(Q_OS_WIN))
    auto kdsa = KDSingleApplication(QStringLiteral("com.phramer.Phramer"));
    QByteArray data;
    QDataStream stream(&data, QIODevice::WriteOnly);
    stream << QStringLiteral("attachTextToClipboard") << text << notification;
    kdsa.sendMessage(data);
#else
    auto m = createMethodCall(QStringLiteral("attachTextToClipboard"));
    m << text << notification;
    call(m);
#endif
}

/**
 * @brief Is this instance of flameshot hosting any windows as a daemon?
 */
bool FlameshotDaemon::isThisInstanceHostingWidgets()
{
    return instance() && !instance()->m_widgets.isEmpty();
}

void FlameshotDaemon::sendTrayNotification(const QString& text,
                                           const QString& title,
                                           const int timeout)
{
    if (m_trayIcon) {
        m_trayIcon->showMessage(title, text, GlobalValues::appIcon(), timeout);
    }
}

#if !defined(DISABLE_UPDATE_CHECKER)
void FlameshotDaemon::showUpdateNotificationIfAvailable(CaptureWidget* widget)
{
    if (!m_appLatestUrl.isEmpty() &&
        ConfigHandler().ignoreUpdateToVersion().compare(m_appLatestVersion) <
          0) {
        widget->showAppUpdateNotification(m_appLatestVersion, m_appLatestUrl);
    }
}

void FlameshotDaemon::getLatestAvailableVersion()
{
    // This features is required for MacOS and Windows user and for Linux users
    // who installed Flameshot not from the repository.
    QNetworkRequest requestCheckUpdates(QUrl(FLAMESHOT_APP_VERSION_URL));
    if (nullptr == m_networkCheckUpdates) {
        m_networkCheckUpdates = new QNetworkAccessManager(this);
        connect(m_networkCheckUpdates,
                &QNetworkAccessManager::finished,
                this,
                &FlameshotDaemon::handleReplyCheckUpdates);
    }
    m_networkCheckUpdates->get(requestCheckUpdates);

    // check for updates each 24 hours
    QTimer::singleShot(1000 * 60 * 60 * 24, [this]() {
        if (ConfigHandler().checkForUpdates()) {
            this->getLatestAvailableVersion();
        }
    });
}

void FlameshotDaemon::checkForUpdates()
{
    if (m_appLatestUrl.isEmpty()) {
        // No update has been seen yet; run a check and report its outcome.
        // Without this the menu entry did nothing at all when automatic
        // checks were enabled and gave the user no indication why.
        m_showManualCheckAppUpdateStatus = true;
        getLatestAvailableVersion();
        return;
    }

    QVersionNumber appLatestVersion =
      QVersionNumber::fromString(m_appLatestVersion);
    if (Flameshot::instance()->getVersion() < appLatestVersion) {
#if defined(Q_OS_WIN)
        startUpdateAndRestart();
#else
        QDesktopServices::openUrl(QUrl(m_appLatestUrl));
#endif
    } else {
        sendTrayNotification(tr("You have the latest version"), "Phramer");
    }
}

#if defined(Q_OS_WIN)
namespace {
// Release asset URLs redirect to a CDN host, so following redirects has to
// be requested rather than assumed
QNetworkRequest assetRequest(const QString& url)
{
    QNetworkRequest request((QUrl(url)));
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                         QNetworkRequest::NoLessSafeRedirectPolicy);
    return request;
}
} // namespace

void FlameshotDaemon::startUpdateAndRestart()
{
    if (m_updateInProgress) {
        return;
    }

    // Without a verifiable installer there is nothing safe to apply;
    // offer the release page instead
    if (m_appLatestMsiUrl.isEmpty() || m_appLatestShaUrl.isEmpty()) {
        QDesktopServices::openUrl(QUrl(m_appLatestUrl));
        return;
    }

    QMessageBox box;
    box.setWindowTitle(QStringLiteral("Phramer"));
    box.setWindowIcon(GlobalValues::appIcon());
    box.setIcon(QMessageBox::Question);
    box.setText(
      tr("Update to version %1 and restart?").arg(m_appLatestVersion));
    box.setInformativeText(
      tr("Phramer will close while the update installs and reopen when it "
         "is done. Windows will ask for administrator approval; if this "
         "account has no administrator rights, administrator credentials "
         "can be entered at that prompt.") +
      "\n\n" + tr("Release notes: %1").arg(m_appLatestUrl));
    QPushButton* updateBtn =
      box.addButton(tr("Update now"), QMessageBox::AcceptRole);
    box.addButton(tr("Later"), QMessageBox::RejectRole);
    QPushButton* skipBtn =
      box.addButton(tr("Skip this version"), QMessageBox::DestructiveRole);
    box.setDefaultButton(updateBtn);
    box.exec();

    if (box.clickedButton() == skipBtn) {
        ConfigHandler().setIgnoreUpdateToVersion(m_appLatestVersion);
        if (m_trayIcon) {
            m_trayIcon->clearUpdateBadge();
        }
        return;
    }
    if (box.clickedButton() != updateBtn) {
        return;
    }

    m_updateInProgress = true;
    downloadUpdateInstaller();
}

void FlameshotDaemon::downloadUpdateInstaller()
{
    if (m_updateDownloader == nullptr) {
        m_updateDownloader = new QNetworkAccessManager(this);
    }

    // The checksum first: it is tiny, and there is no point downloading an
    // installer that could never be verified
    auto* shaReply = m_updateDownloader->get(assetRequest(m_appLatestShaUrl));
    connect(shaReply, &QNetworkReply::finished, this, [this, shaReply]() {
        shaReply->deleteLater();
        if (shaReply->error() != QNetworkReply::NoError) {
            failUpdate(tr("Could not download the update checksum."));
            return;
        }
        m_expectedMsiSha256 = QString::fromLatin1(shaReply->readAll())
                                .trimmed()
                                .section(' ', 0, 0)
                                .toLower();
        if (m_expectedMsiSha256.size() != 64) {
            failUpdate(tr("The update checksum file is malformed."));
            return;
        }

        auto* msiReply =
          m_updateDownloader->get(assetRequest(m_appLatestMsiUrl));

        auto* progress = new QProgressDialog(
          tr("Downloading Phramer update…"), tr("Cancel"), 0, 100);
        progress->setWindowTitle(QStringLiteral("Phramer"));
        progress->setWindowModality(Qt::ApplicationModal);
        progress->setMinimumDuration(0);
        progress->setAttribute(Qt::WA_DeleteOnClose);
        connect(progress,
                &QProgressDialog::canceled,
                msiReply,
                &QNetworkReply::abort);
        connect(msiReply,
                &QNetworkReply::downloadProgress,
                progress,
                [progress](qint64 received, qint64 total) {
                    if (total > 0) {
                        progress->setValue(
                          static_cast<int>(received * 100 / total));
                    }
                });

        connect(
          msiReply,
          &QNetworkReply::finished,
          this,
          [this, msiReply, progress]() {
              msiReply->deleteLater();
              progress->close();
              if (msiReply->error() != QNetworkReply::NoError) {
                  failUpdate(
                    msiReply->error() == QNetworkReply::OperationCanceledError
                      ? tr("Update canceled.")
                      : tr("Could not download the update installer."));
                  return;
              }

              const QByteArray installer = msiReply->readAll();
              const QString actualSha = QString::fromLatin1(
                QCryptographicHash::hash(installer, QCryptographicHash::Sha256)
                  .toHex());
              if (actualSha != m_expectedMsiSha256) {
                  // A mismatched installer must never be offered for
                  // execution
                  failUpdate(
                    tr("The downloaded installer failed verification and "
                       "was discarded."));
                  return;
              }

              const QString msiPath =
                QStandardPaths::writableLocation(QStandardPaths::TempLocation) +
                QStringLiteral("/Phramer-%1-win64.msi").arg(m_appLatestVersion);
              QFile msiFile(msiPath);
              if (!msiFile.open(QIODevice::WriteOnly) ||
                  msiFile.write(installer) != installer.size()) {
                  failUpdate(tr("Could not save the installer to disk."));
                  return;
              }
              msiFile.close();

              applyUpdate(msiPath);
          });
    });
}

void FlameshotDaemon::applyUpdate(const QString& msiPath)
{
    // A batch script chains install and relaunch: cmd waits for msiexec,
    // then starts the app from its (unchanged) install path. Run with
    // plain '&' so a decline at the UAC prompt still brings the old
    // version back instead of leaving nothing running.
    const QString scriptPath =
      QStandardPaths::writableLocation(QStandardPaths::TempLocation) +
      QStringLiteral("/phramer-update.cmd");
    QFile script(scriptPath);
    if (!script.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        failUpdate(tr("Could not prepare the update script."));
        return;
    }
    const QString exePath =
      QDir::toNativeSeparators(QCoreApplication::applicationFilePath());
    QTextStream out(&script);
    out << "@echo off\r\n"
        << "msiexec /i \"" << QDir::toNativeSeparators(msiPath)
        << "\" /passive /norestart\r\n"
        << "start \"\" \"" << exePath << "\"\r\n"
        << "del \"%~f0\"\r\n";
    script.close();

    m_updateInProgress = false;
    QProcess::startDetached(QStringLiteral("cmd.exe"),
                            { QStringLiteral("/C"), scriptPath });
    qApp->quit();
}

void FlameshotDaemon::failUpdate(const QString& reason)
{
    m_updateInProgress = false;
    sendTrayNotification(reason, QStringLiteral("Phramer"));
}
#endif
#endif

/**
 * @brief Return the daemon instance.
 *
 * If this instance of flameshot is the daemon, a singleton instance of
 * `FlameshotDaemon` is returned. As a side effect`start` will called if it
 * wasn't called earlier. If this instance of flameshot is not the daemon,
 * `nullptr` is returned.
 *
 * This strategy is used because the daemon needs to receive signals from D-Bus,
 * for which an instance of a `QObject` is required. The singleton serves as
 * that object.
 */
FlameshotDaemon* FlameshotDaemon::instance()
{
    // Because we don't use DBus on MacOS, each instance of flameshot is its own
    // mini-daemon, responsible for hosting its own persistent widgets (e.g.
    // pins).
#if defined(Q_OS_MACOS)
    start();
#endif
    return m_instance;
}

/**
 * @brief Quit the daemon if it has nothing to do and the 'persist' flag is not
 * set.
 */
void FlameshotDaemon::quitIfIdle()
{
    if (m_persist) {
        return;
    }
    if (!m_hostingClipboard && m_widgets.isEmpty()) {
        qApp->exit(E_OK);
    }
}

// SERVICE METHODS

void FlameshotDaemon::attachPin(const QPixmap& pixmap, QRect geometry)
{
    auto* pinWidget = new PinWidget(pixmap, geometry);
    m_widgets.append(pinWidget);
    connect(pinWidget, &QObject::destroyed, this, [=, this]() {
        m_widgets.removeOne(pinWidget);
        quitIfIdle();
    });

    pinWidget->show();
    pinWidget->activateWindow();
}

void FlameshotDaemon::attachScreenshotToClipboard(const QPixmap& pixmap)
{
    m_hostingClipboard = true;
    QClipboard* clipboard = QApplication::clipboard();
    clipboard->blockSignals(true);
    // This variable is necessary because the signal doesn't get blocked on
    // windows for some reason
    m_clipboardSignalBlocked = true;
    saveToClipboard(pixmap);
    clipboard->blockSignals(false);
}

// D-BUS / KDSingleApplication METHODS

void FlameshotDaemon::attachPin(const QByteArray& data)
{
    QDataStream stream(data);
    QPixmap pixmap;
    QRect geometry;

    stream >> pixmap;
    stream >> geometry;

    attachPin(pixmap, geometry);
}

void FlameshotDaemon::attachScreenshotToClipboard(const QByteArray& screenshot)
{
    QDataStream stream(screenshot);
    QPixmap p;
    stream >> p;

    attachScreenshotToClipboard(p);
}

void FlameshotDaemon::attachTextToClipboard(const QString& text,
                                            const QString& notification)
{
    // Must send notification before clipboard modification on linux
    if (!notification.isEmpty()) {
        AbstractLogger::info() << notification;
    }

    m_hostingClipboard = true;
    QClipboard* clipboard = QApplication::clipboard();

    clipboard->blockSignals(true);
    // This variable is necessary because the signal doesn't get blocked on
    // windows for some reason
    m_clipboardSignalBlocked = true;
    clipboard->setText(text);
    clipboard->blockSignals(false);
}

void FlameshotDaemon::initTrayIcon()
{
    if (!ConfigHandler().disabledTrayIcon()) {
        enableTrayIcon(true);
    }
#if defined(Q_OS_WIN)
    GlobalShortcutFilter* nativeFilter = new GlobalShortcutFilter(this);
    qApp->installNativeEventFilter(nativeFilter);
#endif
}

void FlameshotDaemon::enableTrayIcon(bool enable)
{
    if (enable) {
        if (m_trayIcon == nullptr) {
            m_trayIcon = new TrayIcon();
        } else {
            m_trayIcon->show();
            return;
        }
    } else if (m_trayIcon) {
        m_trayIcon->hide();
    }
}

#if !defined(DISABLE_UPDATE_CHECKER)
void FlameshotDaemon::handleReplyCheckUpdates(QNetworkReply* reply)
{
    if (!ConfigHandler().checkForUpdates() &&
        !m_showManualCheckAppUpdateStatus) {
        return;
    }

    if (reply->error() == QNetworkReply::NoError) {
        QJsonDocument response = QJsonDocument::fromJson(reply->readAll());
        QJsonObject json = response.object();
        m_appLatestVersion = json["tag_name"].toString().replace("v", "");

        QVersionNumber appLatestVersion =
          QVersionNumber::fromString(m_appLatestVersion);
        if (Flameshot::instance()->getVersion() < appLatestVersion) {
            m_appLatestUrl = json["html_url"].toString();
#if defined(Q_OS_WIN)
            // Collect the installer and its checksum; both are required
            // for the in-place update flow, otherwise only the release
            // page fallback is offered
            m_appLatestMsiUrl.clear();
            m_appLatestShaUrl.clear();
            for (const QJsonValue& asset : json["assets"].toArray()) {
                const QString name = asset["name"].toString();
                const QString url = asset["browser_download_url"].toString();
                if (name.endsWith(QLatin1String(".msi"), Qt::CaseInsensitive)) {
                    m_appLatestMsiUrl = url;
                } else if (name.endsWith(QLatin1String(".msi.sha256sum"),
                                         Qt::CaseInsensitive)) {
                    m_appLatestShaUrl = url;
                }
            }
#endif
            emit newVersionAvailable(appLatestVersion);
            if (m_showManualCheckAppUpdateStatus) {
#if defined(Q_OS_WIN)
                startUpdateAndRestart();
#else
                QDesktopServices::openUrl(QUrl(m_appLatestUrl));
#endif
            }
        } else if (m_showManualCheckAppUpdateStatus) {
            sendTrayNotification(tr("You have the latest version"), "Phramer");
        }
    } else {
        qWarning() << "Failed to get information about the latest version. "
                   << reply->errorString();
        if (m_showManualCheckAppUpdateStatus) {
            if (FlameshotDaemon::instance()) {
                FlameshotDaemon::instance()->sendTrayNotification(
                  tr("Failed to get information about the latest version."),
                  "Phramer");
            }
        }
    }
    m_showManualCheckAppUpdateStatus = false;
}
#endif

#if !(defined(Q_OS_MACOS) || defined(Q_OS_WIN))
QDBusMessage FlameshotDaemon::createMethodCall(const QString& method)
{
    QDBusMessage m =
      QDBusMessage::createMethodCall(QStringLiteral("com.phramer.Phramer"),
                                     QStringLiteral("/"),
                                     QLatin1String(""),
                                     method);
    return m;
}

void FlameshotDaemon::checkDBusConnection(const QDBusConnection& connection)
{
    if (!connection.isConnected()) {
        AbstractLogger::error() << tr("Unable to connect via DBus");
        qApp->exit(E_DBUSCONN);
    }
}

void FlameshotDaemon::call(const QDBusMessage& m)
{
    QDBusConnection sessionBus = QDBusConnection::sessionBus();
    checkDBusConnection(sessionBus);
    sessionBus.call(m);
}
#endif

#if defined(USE_KDSINGLEAPPLICATION) &&                                        \
  (defined(Q_OS_MACOS) || defined(Q_OS_WIN))
void FlameshotDaemon::messageReceivedFromSecondaryInstance(
  const QByteArray& message)
{
    // qDebug() << "Received message from second instance:" << message;

    QByteArray messageCopy = message;
    QBuffer buffer(&messageCopy);
    buffer.open(QIODevice::ReadOnly);
    QDataStream stream(&buffer);
    QString methodCall;
    stream >> methodCall;
    // qDebug() << "Method:" << methodCall;

    if (methodCall == QStringLiteral("attachPin")) {
        QPixmap capture;
        QRect geometry;
        stream >> capture >> geometry;
        // qDebug() << "Pixmap:" << capture;
        // qDebug() << "Geometry:" << geometry;
        if (!capture.isNull()) {
            FlameshotDaemon::instance()->attachPin(capture, geometry);
        } else {
            qWarning() << "Received \"attachPin\" from second instance, but "
                          "pixmap is empty!";
        }
    } else if (methodCall == QStringLiteral("attachScreenshotToClipboard")) {
        QPixmap capture;
        stream >> capture;
        // qDebug() << "Pixmap:" << capture;
        if (!capture.isNull()) {
            FlameshotDaemon::instance()->attachScreenshotToClipboard(capture);
        } else {
            qWarning() << "Received \"attachScreenshotToClipboard\" from "
                          "second instance, but pixmap is empty!";
        }
    } else if (methodCall == (QStringLiteral("attachTextToClipboard"))) {
        QString text;
        QString notification;
        stream >> text >> notification;
        // qDebug() << "Text:" << text;
        // qDebug() << "Notification:" << notification;
        if (!text.isEmpty()) {
            FlameshotDaemon::instance()->attachTextToClipboard(text,
                                                               notification);
        } else {
            qWarning() << "Received \"attachTextToClipboard\" from second "
                          "instance, but text is empty!";
        }
    } else {
        qWarning() << "Received unknown message from second instance:"
                   << message;
    }
}
#endif

// STATIC ATTRIBUTES
FlameshotDaemon* FlameshotDaemon::m_instance = nullptr;
