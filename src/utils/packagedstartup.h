// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#if defined(Q_OS_WIN) || defined(_WIN32)

/**
 * @brief Launch at sign-in for the MSIX (Store) edition.
 *
 * A package cannot use the Run key: its HKCU writes go to a private copy
 * that Explorer never reads, and its install path changes with every
 * version. Instead the manifest declares a startup task (TaskId
 * "PhramerStartup", packaging/msix/AppxManifest.xml.in) and this switches
 * it through Windows.ApplicationModel.StartupTask.
 *
 * The user stays in charge: one who turns the task off in Task Manager or
 * Settings > Apps > Startup cannot have it turned back on by the app, only
 * by going there again.
 */
namespace PackagedStartup {

enum class State
{
    // No package identity, or no task declared
    Unavailable,
    Disabled,
    // Turned off by the user outside the app; only they can turn it back on
    DisabledByUser,
    DisabledByPolicy,
    Enabled
};

State state();

// Returns the state afterwards, which differs from the request when the
// user or policy has the last word
State setEnabled(bool enable);

// Settings > Apps > Startup, where a task the user disabled is re-enabled
void openStartupSettings();

} // namespace PackagedStartup

#endif
