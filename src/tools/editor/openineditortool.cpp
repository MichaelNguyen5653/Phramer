// SPDX-License-Identifier: GPL-3.0-or-later

#include "openineditortool.h"

OpenInEditorTool::OpenInEditorTool(QObject* parent)
  : AbstractActionTool(parent)
{}

bool OpenInEditorTool::closeOnButtonPressed() const
{
    return true;
}

QIcon OpenInEditorTool::icon(const QColor& background, bool inEditor) const
{
    Q_UNUSED(inEditor)
    return QIcon(iconPath(background) + "open-in-editor.svg");
}

QString OpenInEditorTool::name() const
{
    return tr("Open in Editor");
}

CaptureTool::Type OpenInEditorTool::type() const
{
    return CaptureTool::TYPE_OPEN_IN_EDITOR;
}

QString OpenInEditorTool::description() const
{
    return tr("Open this capture in the editor");
}

CaptureTool* OpenInEditorTool::copy(QObject* parent)
{
    return new OpenInEditorTool(parent);
}

void OpenInEditorTool::pressed(CaptureContext& context)
{
    emit requestAction(REQ_CLEAR_SELECTION);
    context.request.addTask(CaptureRequest::OPEN_IN_EDITOR);
    emit requestAction(REQ_CAPTURE_DONE_OK);
    emit requestAction(REQ_CLOSE_GUI);
}
