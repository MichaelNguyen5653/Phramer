// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/screenclipprotocol.h"

#include <QCoreApplication>
#include <QDesktopServices>
#include <QDir>
#include <QRegularExpression>
#include <QSettings>
#include <QUrl>

#include <windows.h>

#include <shellapi.h>

namespace {

// Every name here is the one the Shortcuts-page registration of 14.x used.
// A user who registered and chose Phramer back then keeps a UserChoice that
// names ProgId "Phramer"; reusing it means that choice stays valid, and is
// recognised, instead of having to be made again.
const QString AppName = QStringLiteral("Phramer");
const QString CapabilitiesPath =
  QStringLiteral("SOFTWARE\\Phramer\\Capabilities");
const QString ProgId = QStringLiteral("Phramer");

const QString MachineSoftware = QStringLiteral("HKEY_LOCAL_MACHINE\\SOFTWARE");
const QString MachineClasses =
  QStringLiteral("HKEY_LOCAL_MACHINE\\SOFTWARE\\Classes");
// 14.x wrote the ProgId per user, without elevation
const QString UserClasses =
  QStringLiteral("HKEY_CURRENT_USER\\Software\\Classes");
// The merged view Windows resolves a ProgId through, per-user over machine
const QString MergedClasses = QStringLiteral("HKEY_CLASSES_ROOT");
const QString RegisteredApps =
  QStringLiteral("HKEY_LOCAL_MACHINE\\SOFTWARE\\RegisteredApplications");
const QString UserChoice =
  QStringLiteral("HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\Shell\\"
                 "Associations\\UrlAssociations\\ms-screenclip\\UserChoice");

const QString CommandValue = QStringLiteral("shell/open/command/Default");
const QString AssociationValue =
  QStringLiteral("Capabilities/URLAssociations/ms-screenclip");

// Test builds of 15.0 wrote a command onto Windows' own ms-screenclip key,
// and then a ProgId of another name. Neither shipped, but both are still
// Phramer's to clean up.
const QString LegacyKey =
  QStringLiteral("HKEY_LOCAL_MACHINE\\SOFTWARE\\Classes\\ms-screenclip");
const QString LegacyProgId = QStringLiteral("Phramer.ScreenClip");

// The UAC prompt can sit unanswered; past this the caller gives up waiting
// and reports a failure rather than hanging the welcome forever
constexpr DWORD ElevatedWaitMs = 120000;

bool isPhramerCommand(const QString& command)
{
    // Exactly the shape command() produces, for any install location, or the
    // "gui" one earlier 15.0 builds registered
    static const QRegularExpression phramerCommand(
      QStringLiteral(R"(^"[^"]*\\phramer\.exe" (gui|--screenclip "%1")$)"),
      QRegularExpression::CaseInsensitiveOption);
    return phramerCommand.match(command).hasMatch();
}

// What Windows would run for the ProgId, wherever it was registered
QString progIdCommand()
{
    const QSettings classes(MergedClasses, QSettings::NativeFormat);
    return classes.value(ProgId + QLatin1Char('/') + CommandValue).toString();
}

// Removes a ProgId from the given classes root, but only one of Phramer's
void removeProgId(const QString& root, const QString& progId)
{
    QSettings classes(root, QSettings::NativeFormat);
    if (isPhramerCommand(
          classes.value(progId + QLatin1Char('/') + CommandValue).toString())) {
        classes.remove(progId);
        classes.sync();
    }
}

void removeLegacyEntry()
{
    removeProgId(MachineClasses, LegacyProgId);

    QSettings legacy(LegacyKey, QSettings::NativeFormat);
    if (!isPhramerCommand(legacy.value(CommandValue).toString())) {
        return;
    }
    legacy.remove(QStringLiteral("shell/open/command"));
    // The parents go too, but only once nothing else lives under them
    for (const QString& group :
         { QStringLiteral("shell/open"), QStringLiteral("shell") }) {
        legacy.beginGroup(group);
        const bool empty =
          legacy.childKeys().isEmpty() && legacy.childGroups().isEmpty();
        legacy.endGroup();
        if (empty) {
            legacy.remove(group);
        }
    }
}

// Starts this exe elevated with one argument and waits for its exit code
ScreenClipProtocol::Result runElevated(const char* argument)
{
    using ScreenClipProtocol::Result;
    const std::wstring file =
      QDir::toNativeSeparators(QCoreApplication::applicationFilePath())
        .toStdWString();
    const std::wstring parameters =
      QString::fromLatin1(argument).toStdWString();

    SHELLEXECUTEINFOW info{};
    info.cbSize = sizeof(info);
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = L"runas";
    info.lpFile = file.c_str();
    info.lpParameters = parameters.c_str();
    info.nShow = SW_HIDE;

    if (!ShellExecuteExW(&info)) {
        return GetLastError() == ERROR_CANCELLED ? Result::Cancelled
                                                 : Result::Failed;
    }
    if (!info.hProcess) {
        return Result::Failed;
    }
    DWORD exitCode = 1;
    const bool finished =
      WaitForSingleObject(info.hProcess, ElevatedWaitMs) == WAIT_OBJECT_0;
    if (finished) {
        GetExitCodeProcess(info.hProcess, &exitCode);
    }
    CloseHandle(info.hProcess);
    return finished && exitCode == 0 ? Result::Succeeded : Result::Failed;
}

} // namespace

namespace ScreenClipProtocol {

QString command()
{
    // The link is passed so a recording request (Win+Shift+R) can be told
    // from a snip and handed back to Snipping Tool. main() reads it before the
    // command-line parser, which never sees it.
    return QStringLiteral("\"%1\" %2 \"%3\"")
      .arg(QDir::toNativeSeparators(QCoreApplication::applicationFilePath()),
           QString::fromLatin1(ActivationArgument),
           QStringLiteral("%1"));
}

bool isRegistered()
{
    const QSettings software(MachineSoftware, QSettings::NativeFormat);
    const QSettings registered(RegisteredApps, QSettings::NativeFormat);
    return progIdCommand().compare(command(), Qt::CaseInsensitive) == 0 &&
           software.value(AppName + QLatin1Char('/') + AssociationValue)
               .toString()
               .compare(ProgId, Qt::CaseInsensitive) == 0 &&
           registered.value(AppName).toString().compare(
             CapabilitiesPath, Qt::CaseInsensitive) == 0;
}

bool isRegisteredByPhramer()
{
    return isPhramerCommand(progIdCommand());
}

bool isDefault()
{
    const QSettings choice(UserChoice, QSettings::NativeFormat);
    return choice.value(QStringLiteral("ProgId"))
             .toString()
             .compare(ProgId, Qt::CaseInsensitive) == 0;
}

bool writeRegistration()
{
    removeLegacyEntry();
    // A per-user ProgId from 14.x would shadow the machine-wide one, and may
    // point at an older install
    removeProgId(UserClasses, ProgId);

    const QString exe =
      QDir::toNativeSeparators(QCoreApplication::applicationFilePath());

    // What runs when ms-screenclip is pointed at Phramer
    QSettings classes(MachineClasses, QSettings::NativeFormat);
    classes.beginGroup(ProgId);
    classes.setValue(QStringLiteral("Default"),
                     QStringLiteral("Phramer screen capture"));
    classes.setValue(QStringLiteral("DefaultIcon/Default"),
                     QStringLiteral("\"%1\",0").arg(exe));
    classes.setValue(CommandValue, command());
    classes.endGroup();
    classes.sync();

    // The Default Programs listing that makes Phramer a choice in Settings
    QSettings software(MachineSoftware, QSettings::NativeFormat);
    software.beginGroup(AppName + QStringLiteral("/Capabilities"));
    software.setValue(QStringLiteral("ApplicationName"), AppName);
    software.setValue(QStringLiteral("ApplicationDescription"),
                      QStringLiteral("Screenshot and annotation tool"));
    software.setValue(QStringLiteral("ApplicationIcon"),
                      QStringLiteral("\"%1\",0").arg(exe));
    software.setValue(QStringLiteral("URLAssociations/ms-screenclip"), ProgId);
    software.endGroup();
    software.sync();

    QSettings registered(RegisteredApps, QSettings::NativeFormat);
    registered.setValue(AppName, CapabilitiesPath);
    registered.sync();

    return classes.status() == QSettings::NoError &&
           software.status() == QSettings::NoError &&
           registered.status() == QSettings::NoError && isRegistered();
}

bool removeRegistration()
{
    removeLegacyEntry();
    // Each part only when it is Phramer's. A 14.x registration has the
    // listing but keeps its ProgId per user.
    removeProgId(UserClasses, ProgId);
    removeProgId(MachineClasses, ProgId);

    QSettings software(MachineSoftware, QSettings::NativeFormat);
    if (software.value(AppName + QLatin1Char('/') + AssociationValue)
          .toString()
          .compare(ProgId, Qt::CaseInsensitive) == 0) {
        software.remove(AppName + QStringLiteral("/Capabilities"));
        software.beginGroup(AppName);
        const bool empty =
          software.childKeys().isEmpty() && software.childGroups().isEmpty();
        software.endGroup();
        if (empty) {
            software.remove(AppName);
        }
        software.sync();
    }

    QSettings registered(RegisteredApps, QSettings::NativeFormat);
    if (registered.value(AppName).toString().compare(
          CapabilitiesPath, Qt::CaseInsensitive) == 0) {
        registered.remove(AppName);
        registered.sync();
    }

    return software.status() == QSettings::NoError &&
           registered.status() == QSettings::NoError &&
           !isRegisteredByPhramer();
}

Result registerElevated()
{
    const Result result = runElevated(RegisterArgument);
    // The elevated copy already did this, unless UAC was answered with
    // another account's credentials: then its HKCU was not this user's
    if (result == Result::Succeeded) {
        removeProgId(UserClasses, ProgId);
    }
    return result;
}

Result unregisterElevated()
{
    const Result result = runElevated(UnregisterArgument);
    if (result == Result::Succeeded) {
        removeProgId(UserClasses, ProgId);
    }
    return result;
}

void openDefaultAppsSettings()
{
    // Phramer's own page exists only while it is registered. Once it is not,
    // the general page is where another app is chosen for ms-screenclip.
    if (!isRegisteredByPhramer()) {
        QDesktopServices::openUrl(
          QUrl(QStringLiteral("ms-settings:defaultapps")));
        return;
    }
    QDesktopServices::openUrl(
      QUrl(QStringLiteral("ms-settings:defaultapps?registeredAppMachine=%1")
             .arg(QString::fromLatin1(QUrl::toPercentEncoding(AppName)))));
}

} // namespace ScreenClipProtocol
