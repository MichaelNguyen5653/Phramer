// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QString>

/**
 * @brief Handing a saved capture to other apps as a file.
 *
 * Teams, Outlook and Explorer treat a pasted image and a pasted file
 * differently: an image is inlined and recompressed, a file arrives as an
 * attachment with its name. These put the file itself on the clipboard, the
 * way Explorer's Copy does, and open Explorer with it selected.
 *
 * Nothing here touches the network; the file stays where it was saved.
 */
namespace FileHandoff {

// Puts `path` on the clipboard as a file, not as image data
void copyFileToClipboard(const QString& path);

// Opens the containing folder with `path` selected. Returns false when the
// file no longer exists.
bool showInFolder(const QString& path);

// The last capture this process saved, for "Show in folder" after the fact.
// Kept in memory only: a list of where screenshots went is not something to
// leave on disk.
void rememberSaved(const QString& path);
QString lastSaved();

} // namespace FileHandoff
