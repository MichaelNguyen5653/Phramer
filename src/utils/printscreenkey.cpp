// SPDX-License-Identifier: GPL-3.0-or-later

#include "printscreenkey.h"

#if defined(Q_OS_WIN) || defined(_WIN32)

#include <QDesktopServices>
#include <QObject>
#include <QSettings>
#include <QUrl>

namespace {
constexpr auto KeyboardKey = "HKEY_CURRENT_USER\\Control Panel\\Keyboard";
constexpr auto SnippingValue = "PrintScreenKeyForSnippingEnabled";
}

namespace PrintScreenKey {

bool isSnippingDisabled()
{
    QSettings settings(KeyboardKey, QSettings::NativeFormat);
    return settings.value(SnippingValue, 1).toInt() == 0;
}

#if defined(PHRAMER_STORE_BUILD)
bool changesSettingDirectly()
{
    return false;
}

bool disableSnipping()
{
    // The supported way to change another part of Windows: take the user to
    // the setting. Store policy 10.2.8 asks for exactly this.
    return QDesktopServices::openUrl(
      QUrl(QStringLiteral("ms-settings:easeofaccess-keyboard")));
}

QString disableInstructions()
{
    return QObject::tr(
      "In the Windows Settings page that opens, turn off \"Use the Print "
      "screen key to open screen capture\", then restart Phramer.");
}
#else
bool changesSettingDirectly()
{
    return true;
}

bool disableSnipping()
{
    QSettings settings(KeyboardKey, QSettings::NativeFormat);
    settings.setValue(SnippingValue, 0);
    settings.sync();
    if (QSettings::AccessError == settings.status()) {
        return false;
    }
    return isSnippingDisabled();
}

QString disableInstructions()
{
    return QObject::tr("Phramer must be restarted for the change to take "
                       "effect.");
}
#endif

} // namespace PrintScreenKey

#endif
