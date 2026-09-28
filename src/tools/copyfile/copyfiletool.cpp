// SPDX-License-Identifier: GPL-3.0-or-later

#include "copyfiletool.h"

CopyFileTool::CopyFileTool(QObject* parent)
  : AbstractActionTool(parent)
{}

bool CopyFileTool::closeOnButtonPressed() const
{
    return true;
}

QIcon CopyFileTool::icon(const QColor& background, bool inEditor) const
{
    Q_UNUSED(inEditor)
    return QIcon(iconPath(background) + "file-copy.svg");
}

QString CopyFileTool::name() const
{
    return tr("Copy as File");
}

CaptureTool::Type CopyFileTool::type() const
{
    return CaptureTool::TYPE_COPY_FILE;
}

QString CopyFileTool::description() const
{
    return tr("Save the selection and copy the file, to paste as an "
              "attachment");
}

CaptureTool* CopyFileTool::copy(QObject* parent)
{
    return new CopyFileTool(parent);
}

void CopyFileTool::pressed(CaptureContext& context)
{
    emit requestAction(REQ_CLEAR_SELECTION);
    context.request.addTask(CaptureRequest::COPY_FILE);
    emit requestAction(REQ_CAPTURE_DONE_OK);
    emit requestAction(REQ_CLOSE_GUI);
}
