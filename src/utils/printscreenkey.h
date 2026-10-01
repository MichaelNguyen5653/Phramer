// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#if defined(Q_OS_WIN) || defined(_WIN32)

#include <QString>

/**
 * @brief Windows' own claim on the Print Screen key.
 *
 * Windows opens its built-in snipping tool on Print Screen unless the user
 * turns that off, which stops Flameshot from ever seeing the key. Both the
 * first-run welcome prompt and the shortcuts settings page offer to turn it
 * off, so the registry access lives here rather than in either of them.
 */
namespace PrintScreenKey {

// True when Windows is *not* claiming the key, i.e. Flameshot can use it
bool isSnippingDisabled();

// Whether disableSnipping() changes the setting itself. The Store edition
// cannot: an MSIX package's HKCU writes land in a private copy that Windows
// never reads, so it opens the Settings page for the user to flip instead.
bool changesSettingDirectly();

// Ask Windows to stop claiming the key. Returns false if the registry
// could not be written. Takes effect after a restart. Where the setting
// cannot be changed directly, opens Windows Settings on it instead.
bool disableSnipping();

// What the user has to do after disableSnipping(), for the dialogs that
// offer it
QString disableInstructions();

} // namespace PrintScreenKey

#endif
