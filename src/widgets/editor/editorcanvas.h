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
 * The image floats on a larger workspace, so an annotation can sit beside
 * it (a callout, an arrow pointing in) rather than only on it. Tool
 * coordinates are workspace coordinates: (0,0) is the workspace corner and the
 * image starts at imageRect().topLeft(). The workspace corner being the
 * pixmap's corner is what lets tools that read pixels, such as blur and
 * invert, keep indexing the pixmap directly. Zoom lives in toImage()/
 * fromImage(); nothing below this class ever learns about it.
 *
 * An export (rendered()) is the image, grown only as far as annotations reach
 * past it. With nothing outside, it is the image exactly as before.
 *
 * The widget is sized to the workspace scaled by the zoom and is meant to live
 * inside a scroll area. The image keeps its device pixel ratio, so annotations
 * drawn at logical coordinates still land on the full-resolution output. That
 * ratio is a separate scale from the zoom and the two must never be combined:
 * the ratio stays inside the pixmap, the zoom stays inside the mapping.
 */
class EditorCanvas : public QWidget
{
    Q_OBJECT
public:
    explicit EditorCanvas(const QPixmap& image, QWidget* parent = nullptr);

    // The original image with every committed annotation baked in, extended
    // to take in annotations that reach past the image. What is saved,
    // copied, pinned and shown in the filmstrip.
    QPixmap rendered() const { return m_exported; }
    QPixmap original() const { return m_original; }
    // Where the image sits on the workspace, in tool coordinates
    QRect imageRect() const { return m_imageRect; }

    QUndoStack* undoStack() { return &m_undoStack; }

    // True while the image holds annotations that have not been written to
    // disk. Drives the editor's close prompt. Undoing back to the last saved
    // state counts as clean again, which is what QUndoStack's clean index is
    // for.
    bool isDirty() const { return !m_undoStack.isClean(); }
    void markSaved();

    // NONE puts the canvas in select/move mode
    void setActiveToolType(CaptureTool::Type type);
    // Zoom mode gives the plain wheel to the zoom instead of the tool size.
    // It is a view mode, so the active tool type stays NONE either way.
    void setZoomMode(bool enabled) { m_zoomMode = enabled; }
    bool zoomMode() const { return m_zoomMode; }
    // A view aid only: drawn over the canvas on screen, never into rendered()
    // or original(), so nothing saved, copied or pinned carries it
    void setGridVisible(bool visible);
    bool gridVisible() const { return m_gridVisible; }
    CaptureTool::Type activeToolType() const { return m_activeToolType; }

    void setDrawColor(const QColor& color);
    // The range the toolbar's size field offers, kept here so the field and
    // the canvas cannot drift apart
    static constexpr int MinToolSize = 1;
    static constexpr int MaxToolSize = 100;

    void setToolSize(int size);
    int toolSize() const { return m_context.toolSize; }

    // Display scale. 1.0 is one image pixel per device-independent pixel.
    qreal zoom() const { return m_zoom; }
    void setZoom(qreal zoom);
    // Steps the zoom by wheel notches, keeping the image point currently under
    // anchor (in widget coordinates) under it afterwards. Returns true when
    // the zoom actually changed.
    bool zoomBy(int notches, const QPoint& anchor);
    // The image point that was under the pointer when zoomBy last changed the
    // scale. The window scrolls it back under the pointer; the canvas cannot,
    // because the scroll bars belong to the scroll area above it.
    QPoint lastZoomAnchor() const { return m_lastZoomAnchor; }

    // Widget coordinates to workspace (tool) coordinates and back. The only
    // two places that know the zoom exists.
    QPoint toImage(const QPoint& widgetPos) const;
    QPoint fromImage(const QPoint& imagePos) const;

    // Used by the undo command; not part of the public editing API
    void restoreObjects(const CaptureToolObjects& objects);

    // Takes over annotations placed on the capture overlay so they stay
    // editable here. offset maps overlay coordinates to image coordinates.
    // They become the starting state: nothing to undo, and not dirty, the
    // same as a flattened capture.
    void adoptObjects(const QList<QPointer<CaptureTool>>& objects,
                      const QPoint& offset);

public slots:
    void deleteSelectedObject();
    void commitActiveTool();

signals:
    // Anything that changes the rendered pixel content, including undo/redo
    void contentChanged();
    // The drawing colour was changed from the canvas itself, so the window's
    // colour swatch can follow
    void drawColorChanged(const QColor& color);
    // Likewise for the size, which the wheel and the keyboard shortcuts both
    // change without going through the toolbar
    void toolSizeChanged(int size);
    // The zoom changed, so the window can update its readout
    void zoomChanged(qreal zoom);
    // A wheel notch asked for a zoom. The canvas cannot honour it alone: the
    // scroll bars that keep the anchor under the pointer belong to the scroll
    // area above it, so the window performs the zoom.
    void zoomRequested(int notches, const QPoint& anchor);
    // Escape was pressed with a tool picked: the window owns the tool
    // buttons, so it is the one that switches back to Select
    void selectModeRequested();
    // A drag on empty space with no tool picked moves the view. The scroll
    // bars belong to the scroll area above, so the window does the moving;
    // delta is how far the pointer went, in screen pixels.
    void panRequested(const QPoint& delta);

protected:
    void paintEvent(QPaintEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void mouseDoubleClickEvent(QMouseEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;
    void changeEvent(QEvent* event) override;

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
    // Resizes the widget to the canvas at the current zoom. Everything that
    // changes either one goes through here.
    void updateCanvasSize();
    void paintGrid(QPainter& painter, const QRect& dirty) const;
    void paintWorkspace(QPainter& painter) const;
    // The workspace's colour, which is also what an export puts behind
    // annotations that reach past the image
    QColor workspaceColor() const;
    // Renders a blur or invert onto the image part of the workspace only
    void processOnImage(const QPointer<CaptureTool>& object,
                        QPixmap& workspace);
    // The flattened workspace cropped to the image and whatever reaches past
    // it
    QPixmap exportFrom(const QPixmap& flattened);

    QPixmap m_original;
    // The workspace with only the image on it, transparent elsewhere: what
    // every render starts from
    QPixmap m_base;
    // What is on screen: the workspace, annotations and selection outline
    QPixmap m_rendered;
    QPixmap m_exported;

    // The whole editable area in tool coordinates; (0,0) at its corner
    QRect m_canvasRect;
    // The image within it, and the same in the pixmaps' physical pixels
    QRect m_imageRect;
    QRect m_imagePhysical;
    qreal m_zoom{ 1.0 };
    QPoint m_lastZoomAnchor;
    bool m_zoomMode{ false };
    bool m_gridVisible{ false };

    CaptureContext m_context;
    CaptureToolObjects m_objects;
    CaptureToolObjects m_objectsBackup;
    QUndoStack m_undoStack;

    // Prototype for the tool type currently chosen in the toolbar. Placed
    // objects are copies of it, exactly as in the capture editor.
    QPointer<CaptureTool> m_toolPrototype;
    // Rate limiting state for touchpad wheel events
    qint64 m_lastWheelMs{ 0 };
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
    // Dragging the view, and where the pointer was last, in screen pixels
    bool m_panning{ false };
    QPoint m_panLast;
    QPoint m_moveStartPos;
    // Where the left button went down, in widget pixels
    QPoint m_pressPos;
    QPoint m_moveGrabOffset;
};
