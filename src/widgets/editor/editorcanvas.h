// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/capturecontext.h"
#include "tools/capturetool.h"
#include "widgets/capture/capturetoolobjects.h"

#include <QPixmap>
#include <QPointer>
#include <QUndoStack>
#include <QWidget>

class ColorPicker;

/**
 * @brief One image being annotated in the standalone editor.
 *
 * This is the editor's counterpart to CaptureWidget's drawing surface: it
 * drives the same CaptureTool objects through the same process()/drawStart()/
 * drawMove()/drawEnd() protocol, but owns exactly one image and no capture
 * machinery. CaptureWidget cannot be reused here because it is effectively a
 * singleton — its destructor exports the capture and OverlayMessage is a
 * static instance parented to it — while the editor needs one independent
 * surface, with its own undo stack, per image in the session.
 *
 * The widget is sized to the image's device-independent size and is meant to
 * live inside a scroll area, so widget coordinates and tool coordinates are
 * the same thing and no transform is threaded through the tools. The image
 * keeps its device pixel ratio, so annotations drawn at logical coordinates
 * still land on the full-resolution output.
 */
class EditorCanvas : public QWidget
{
    Q_OBJECT
public:
    explicit EditorCanvas(const QPixmap& image, QWidget* parent = nullptr);

    // The original image with every committed annotation baked in
    QPixmap rendered() const { return m_rendered; }
    QPixmap original() const { return m_original; }

    QUndoStack* undoStack() { return &m_undoStack; }

    // True while the image holds annotations that have not been written to
    // disk. Drives the editor's close prompt. Undoing back to the last saved
    // state counts as clean again, which is what QUndoStack's clean index is
    // for.
    bool isDirty() const { return !m_undoStack.isClean(); }
    void markSaved();

    // NONE puts the canvas in select/move mode
    void setActiveToolType(CaptureTool::Type type);
    CaptureTool::Type activeToolType() const { return m_activeToolType; }

    void setDrawColor(const QColor& color);
    void setToolSize(int size);
    int toolSize() const { return m_context.toolSize; }

    // Used by the undo command; not part of the public editing API
    void restoreObjects(const CaptureToolObjects& objects);

public slots:
    void deleteSelectedObject();
    void commitActiveTool();

signals:
    // Anything that changes the rendered pixel content, including undo/redo
    void contentChanged();
    // The drawing colour was changed from the canvas itself, so the window's
    // colour swatch can follow
    void drawColorChanged(const QColor& color);

protected:
    void paintEvent(QPaintEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void mouseDoubleClickEvent(QMouseEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;

private:
    void handleToolSignal(CaptureTool::Request request);
    // Radial colour picker on right click, the same one the capture overlay
    // uses, so the two editors behave the same way
    void showColorPicker(const QPoint& pos);
    bool startDrawing(const QPoint& pos);
    void pushActiveToolToStack();
    void pushObjectsStateToUndoStack();
    void releaseActiveTool();
    void renderObjects();
    void selectObjectAt(const QPoint& pos);
    void updateCursor();
    void restoreCircleCountState();
    QPointer<CaptureTool> selectedObject();

    QPixmap m_original;
    QPixmap m_rendered;

    CaptureContext m_context;
    CaptureToolObjects m_objects;
    CaptureToolObjects m_objectsBackup;
    QUndoStack m_undoStack;

    // Prototype for the tool type currently chosen in the toolbar. Placed
    // objects are copies of it, exactly as in the capture editor.
    QPointer<CaptureTool> m_toolPrototype;
    CaptureTool::Type m_activeToolType{ CaptureTool::NONE };

    // The object currently being drawn or edited
    QPointer<CaptureTool> m_activeTool;
    // Editor widget owned by the active tool, e.g. the text box
    QPointer<QWidget> m_toolWidget;
    ColorPicker* m_colorPicker{ nullptr };

    int m_selectedIndex{ -1 };
    bool m_mousePressed{ false };
    bool m_movingObject{ false };
    bool m_moveStarted{ false };
    QPoint m_moveStartPos;
    QPoint m_moveGrabOffset;
};
