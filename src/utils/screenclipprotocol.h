// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#if defined(Q_OS_WIN) || defined(_WIN32)

#include <QString>

/**
 * @brief Offering Phramer as a handler of the ms-screenclip: protocol.
 *
 * Windows launches ms-screenclip: for its own screen capture (the Print
 * Screen key, and apps that ask for a snip). Which app answers is the user's
 * per-user choice, kept in a hash-protected UserChoice key that no program
 * can write. So Phramer can only put itself on the list of candidates, the
 * Default Programs way, and then open Windows Settings where the user picks
 * it.
 *
 * Being listed means writing under HKLM, which needs administrator rights,
 * so the writes happen in a second, elevated copy of Phramer started with
 * RegisterArgument or UnregisterArgument. That copy changes the keys and
 * exits; nothing else ever runs elevated.
 */
namespace ScreenClipProtocol {

inline constexpr char RegisterArgument[] = "--register-screenclip";
inline constexpr char UnregisterArgument[] = "--unregister-screenclip";

enum class Result
{
    Succeeded,
    Cancelled,
    Failed
};

// The command registered for this copy of Phramer: starts a capture
QString command();

// Whether this copy of Phramer is on the list of ms-screenclip handlers
bool isRegistered();

// Whether any copy of Phramer is on the list, this one or another install.
// Only such a registration is ever removed.
bool isRegisteredByPhramer();

// Whether the user has picked Phramer for ms-screenclip in Windows Settings
bool isDefault();

// The elevated halves: change the keys. Return false if they could not.
bool writeRegistration();
bool removeRegistration();

// The unelevated halves: start the elevated copy through UAC and wait for
// it. Block for as long as the UAC prompt is up.
Result registerElevated();
Result unregisterElevated();

// Opens Phramer's page in Settings > Default apps, where ms-screenclip can
// be pointed at it
void openDefaultAppsSettings();

} // namespace ScreenClipProtocol

#endif
