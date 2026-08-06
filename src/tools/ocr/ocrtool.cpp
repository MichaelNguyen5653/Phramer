// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrtool.h"

#include "tools/ocr/ocrresultswindow.h"

OcrTool::OcrTool(QObject* parent)
  : AbstractActionTool(parent)
{}

bool OcrTool::closeOnButtonPressed() const
{
    return false;
}

QIcon OcrTool::icon(const QColor& background, bool inEditor) const
{
    Q_UNUSED(inEditor)
    return QIcon(iconPath(background) + "ocr.svg");
}

QString OcrTool::name() const
{
    return tr("OCR");
}

CaptureTool::Type OcrTool::type() const
{
    return CaptureTool::TYPE_OCR;
}

QString OcrTool::description() const
{
    return tr("Extract text from the selection");
}

QWidget* OcrTool::widget()
{
    return new OcrResultsWindow(m_capture, m_captureScreenRect);
}

CaptureTool* OcrTool::copy(QObject* parent)
{
    return new OcrTool(parent);
}

void OcrTool::pressed(CaptureContext& context)
{
    // Recognize the untouched screenshot, not selectedScreenshotArea(): that
    // returns the edited pixmap, so any arrow, blur or text already drawn
    // over the selection would be handed to the recognizer as if it were
    // part of the page
    m_capture = context.selection.isNull()
                  ? context.origScreenshot
                  : context.origScreenshot.copy(context.selection);

    // selection is in device pixels relative to the overlay, and the
    // overlay's own origin is only knowable in physical screen space from
    // its native handle — see utils/screencoordinates.h
    const QRect selection =
      context.selection.isNull()
        ? QRect(QPoint(0, 0), context.origScreenshot.size())
        : context.selection;
    m_captureScreenRect = selection.translated(context.widgetScreenOffset);
    // Unlike most action tools this does not close the editor: the results
    // window lives on its own and the user may keep annotating
    emit requestAction(REQ_ADD_EXTERNAL_WIDGETS);
}
