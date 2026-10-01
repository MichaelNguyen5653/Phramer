// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#if defined(Q_OS_WIN) || defined(_WIN32)

#include <QString>

/**
 * @brief Whether this process runs from an MSIX package, and as what.
 *
 * The Store edition is an MSIX package. Windows gives such a process a
 * package identity, redirects its writes to HKCU and new AppData files to a
 * private per-package store, and keeps its install folder read-only. The
 * same binary can still be started outside a package (a developer running
 * the build tree), so behaviour that only works with an identity asks here
 * rather than assuming it from the build.
 */
namespace PackageIdentity {

// True when the process has a package identity
bool isPackaged();

// The package family name, e.g. "Publisher.App_hash"; empty when unpackaged
QString familyName();

// The application user model ID Windows activates this app by, e.g.
// "Publisher.App_hash!Phramer"; empty when unpackaged
QString appUserModelId();

// Starts this app again through Windows' app activation, which is how a
// packaged app keeps its identity across a relaunch. Returns false when
// unpackaged or when activation fails.
bool activateSelf();

} // namespace PackageIdentity

#endif
