// SPDX-License-Identifier: GPL-3.0-or-later

#include "moveobjecttool.h"

MoveObjectTool::MoveObjectTool(QObject* parent)
  : AbstractActionTool(parent)
{}

bool MoveObjectTool::closeOnButtonPressed() const
{
    return false;
}

bool MoveObjectTool::isSelectable() const
{
    return true;
}

QIcon MoveObjectTool::icon(const QColor& background, bool inEditor) const
{
    Q_UNUSED(inEditor)
    return QIcon(iconPath(background) + "pan-tool.svg");
}

QString MoveObjectTool::name() const
{
    return tr("Move Object");
}

CaptureTool::Type MoveObjectTool::type() const
{
    return CaptureTool::TYPE_MOVE_OBJECT;
}

QString MoveObjectTool::description() const
{
    return tr("Select and move objects you have already drawn");
}

CaptureTool* MoveObjectTool::copy(QObject* parent)
{
    return new MoveObjectTool(parent);
}

// The mode does all its work through CaptureWidget's mouse handling, so
// activating the button is the whole action.
void MoveObjectTool::pressed(CaptureContext& context)
{
    Q_UNUSED(context)
}
