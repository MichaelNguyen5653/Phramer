// SPDX-License-Identifier: GPL-3.0-or-later
// SPDX-FileCopyrightText: 2017-2019 Alejandro Sirgo Rica & Contributors

#include "toolfactory.h"

#include "tools/accept/accepttool.h"
#include "tools/arrow/arrowtool.h"
#include "tools/circle/circletool.h"
#include "tools/circlecount/circlecounttool.h"
#include "tools/copy/copytool.h"
#include "tools/editor/openineditortool.h"
#include "tools/exit/exittool.h"
#include <QKeySequence>
#ifdef ENABLE_IMGUR
#include "tools/imgupload/imguploadertool.h"
#endif
#include "tools/invert/inverttool.h"
#include "tools/launcher/applaunchertool.h"
#include "tools/line/linetool.h"
#include "tools/marker/markertool.h"
#include "tools/move/movetool.h"
#include "tools/moveobject/moveobjecttool.h"
#if defined(Q_OS_WIN)
#include "tools/ocr/ocrtool.h"
#endif
#include "tools/pencil/penciltool.h"
#include "tools/pin/pintool.h"
#include "tools/pixelate/pixelatetool.h"
#include "tools/rectangle/rectangletool.h"
#include "tools/redo/redotool.h"
#include "tools/save/savetool.h"
#include "tools/selection/selectiontool.h"
#include "tools/shape/shapetool.h"
#include "tools/sizedecrease/sizedecreasetool.h"
#include "tools/sizeincrease/sizeincreasetool.h"
#include "tools/text/texttool.h"
#include "tools/undo/undotool.h"

ToolFactory::ToolFactory(QObject* parent)
  : QObject(parent)
{}

CaptureTool* ToolFactory::CreateTool(CaptureTool::Type t, QObject* parent)
{
#define if_TYPE_return_TOOL(TYPE, TOOL)                                        \
    case CaptureTool::TYPE:                                                    \
        return new TOOL(parent)

    switch (t) {
        if_TYPE_return_TOOL(TYPE_PENCIL, PencilTool);
        if_TYPE_return_TOOL(TYPE_DRAWER, LineTool);
        if_TYPE_return_TOOL(TYPE_ARROW, ArrowTool);
        if_TYPE_return_TOOL(TYPE_SELECTION, SelectionTool);
        if_TYPE_return_TOOL(TYPE_RECTANGLE, RectangleTool);
        if_TYPE_return_TOOL(TYPE_CIRCLE, CircleTool);
        if_TYPE_return_TOOL(TYPE_MARKER, MarkerTool);
        if_TYPE_return_TOOL(TYPE_MOVESELECTION, MoveTool);
        if_TYPE_return_TOOL(TYPE_MOVE_OBJECT, MoveObjectTool);
        if_TYPE_return_TOOL(TYPE_UNDO, UndoTool);
        if_TYPE_return_TOOL(TYPE_COPY, CopyTool);
        if_TYPE_return_TOOL(TYPE_SAVE, SaveTool);
        if_TYPE_return_TOOL(TYPE_EXIT, ExitTool);
        if_TYPE_return_TOOL(TYPE_OPEN_IN_EDITOR, OpenInEditorTool);
        if_TYPE_return_TOOL(TYPE_SHAPE, ShapeTool);
#ifdef ENABLE_IMGUR
        if_TYPE_return_TOOL(TYPE_IMAGEUPLOADER, ImgUploaderTool);
#endif
#if !defined(Q_OS_MACOS)
        if_TYPE_return_TOOL(TYPE_OPEN_APP, AppLauncher);
#endif
        if_TYPE_return_TOOL(TYPE_PIXELATE, PixelateTool);
        if_TYPE_return_TOOL(TYPE_REDO, RedoTool);
        if_TYPE_return_TOOL(TYPE_PIN, PinTool);
        if_TYPE_return_TOOL(TYPE_TEXT, TextTool);
        if_TYPE_return_TOOL(TYPE_CIRCLECOUNT, CircleCountTool);
        if_TYPE_return_TOOL(TYPE_SIZEINCREASE, SizeIncreaseTool);
        if_TYPE_return_TOOL(TYPE_SIZEDECREASE, SizeDecreaseTool);
        if_TYPE_return_TOOL(TYPE_INVERT, InvertTool);
        if_TYPE_return_TOOL(TYPE_ACCEPT, AcceptTool);
#if defined(Q_OS_WIN)
        if_TYPE_return_TOOL(TYPE_OCR, OcrTool);
#endif
        default:
            return nullptr;
    }
}

QString ToolFactory::shortName(CaptureTool::Type t)
{
    switch (t) {
        case CaptureTool::TYPE_PENCIL:
            return tr("Pencil");
        case CaptureTool::TYPE_DRAWER:
            return tr("Line");
        case CaptureTool::TYPE_ARROW:
            return tr("Arrow");
        case CaptureTool::TYPE_SELECTION:
            return tr("Outline");
        case CaptureTool::TYPE_RECTANGLE:
            return tr("Box");
        case CaptureTool::TYPE_CIRCLE:
            return tr("Circle");
        case CaptureTool::TYPE_MARKER:
            return tr("Marker");
        case CaptureTool::TYPE_MOVESELECTION:
            return tr("Move");
        case CaptureTool::TYPE_UNDO:
            return tr("Undo");
        case CaptureTool::TYPE_COPY:
            return tr("Copy");
        case CaptureTool::TYPE_SAVE:
            return tr("Save");
        case CaptureTool::TYPE_EXIT:
            return tr("Close");
#ifdef ENABLE_IMGUR
        case CaptureTool::TYPE_IMAGEUPLOADER:
            return tr("Upload");
#endif
        case CaptureTool::TYPE_OPEN_APP:
            return tr("Open");
        case CaptureTool::TYPE_PIXELATE:
            return tr("Blur");
        case CaptureTool::TYPE_REDO:
            return tr("Redo");
        case CaptureTool::TYPE_PIN:
            return tr("Pin");
        case CaptureTool::TYPE_TEXT:
            return tr("Text");
        case CaptureTool::TYPE_CIRCLECOUNT:
            return tr("Number");
        case CaptureTool::TYPE_SIZEINCREASE:
            return tr("Bigger");
        case CaptureTool::TYPE_SIZEDECREASE:
            return tr("Smaller");
        case CaptureTool::TYPE_INVERT:
            return tr("Invert");
        case CaptureTool::TYPE_ACCEPT:
            return tr("Done");
        case CaptureTool::TYPE_CANCEL:
            return tr("Cancel");
        case CaptureTool::TYPE_OCR:
            return tr("OCR");
        case CaptureTool::TYPE_OPEN_IN_EDITOR:
            return tr("Editor");
        case CaptureTool::TYPE_SHAPE:
            return tr("Shape");
        case CaptureTool::TYPE_MOVE_OBJECT:
            return tr("Hand");
        default:
            return {};
    }
}

QString ToolFactory::labelWithShortcut(CaptureTool::Type t,
                                       const QString& shortcut,
                                       const QString& name)
{
    const QString base = name.isEmpty() ? shortName(t) : name;
    if (base.isEmpty() || shortcut.trimmed().isEmpty()) {
        return base;
    }
    QStringList keys;
    for (const QString& part : shortcut.split(QLatin1Char('/'))) {
        const QKeySequence sequence(part.trimmed());
        if (sequence.count() != 1 ||
            sequence[0].keyboardModifiers() != Qt::NoModifier) {
            return base;
        }
        QString text = sequence.toString(QKeySequence::NativeText);
        // Same wording the tooltips use
        if (sequence[0].key() == Qt::Key_Return) {
            text = tr("Enter");
        }
        keys << text;
    }
    return QStringLiteral("%1 (%2)").arg(base, keys.join(QLatin1Char('/')));
}
