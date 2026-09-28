// SPDX-License-Identifier: GPL-3.0-or-later

#include "filehandoff.h"

#include <QApplication>
#include <QClipboard>
#include <QDesktopServices>
#include <QDir>
#include <QFileInfo>
#include <QMimeData>
#include <QUrl>

#if defined(Q_OS_WIN)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

#include <shlobj.h>
#endif

namespace {

QString& lastSavedPath()
{
    static QString path;
    return path;
}

} // namespace

namespace FileHandoff {

void copyFileToClipboard(const QString& path)
{
    auto* data = new QMimeData();
    // Qt turns a local-file URL into CF_HDROP, which is what Explorer,
    // Teams and Outlook read as "a file was copied"
    data->setUrls({ QUrl::fromLocalFile(QFileInfo(path).absoluteFilePath()) });
#if defined(Q_OS_WIN)
    // Without this Explorer is free to treat a paste as a move
    QByteArray effect(4, '\0');
    effect[0] = static_cast<char>(DROPEFFECT_COPY);
    data->setData(QStringLiteral("application/x-qt-windows-mime;value="
                                 "\"Preferred DropEffect\""),
                  effect);
#endif
    QApplication::clipboard()->setMimeData(data);
}

bool showInFolder(const QString& path)
{
    const QFileInfo file(path);
    if (!file.exists()) {
        return false;
    }
#if defined(Q_OS_WIN)
    // The shell call, not "explorer /select,<path>": no command line to
    // quote, and it reuses an Explorer window already showing the folder
    const std::wstring native =
      QDir::toNativeSeparators(file.absoluteFilePath()).toStdWString();
    if (PIDLIST_ABSOLUTE item = ILCreateFromPathW(native.c_str())) {
        const HRESULT result = SHOpenFolderAndSelectItems(item, 0, nullptr, 0);
        ILFree(item);
        if (SUCCEEDED(result)) {
            return true;
        }
    }
#endif
    return QDesktopServices::openUrl(QUrl::fromLocalFile(file.absolutePath()));
}

void rememberSaved(const QString& path)
{
    lastSavedPath() = path;
}

QString lastSaved()
{
    return lastSavedPath();
}

} // namespace FileHandoff
