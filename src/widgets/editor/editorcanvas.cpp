// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/editor/editorcanvas.h"

#include "core/qguiappcurrentscreen.h"
#include "tools/toolfactory.h"
#include "utils/confighandler.h"
#include "widgets/capture/colorpicker.h"

#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QScreen>
#include <QUndoCommand>

// Matches CaptureWidget: an object only starts moving once the drag clears
// this many pixels, so a click that selects does not also nudge
#define MOUSE_DISTANCE_TO_START_MOVING 3

namespace {

/**
 * @brief Whole-object-list snapshot, the editor's undo unit.
 *
 * Mirrors ModificationCommand, which cannot be reused because its constructor
 * takes a CaptureWidget.
 */
class CanvasCommand : public QUndoCommand
{
public:
    CanvasCommand(EditorCanvas* canvas,
                  const CaptureToolObjects& objects,
                  const CaptureToolObjects& backup)
      : m_canvas(canvas)
    {
        // CaptureToolObjects is a QObject, so it has no copy constructor;
        // only its deep-copying assignment operator can be used
        m_objects = objects;
        m_backup = backup;
    }

    void undo() override { m_canvas->restoreObjects(m_backup); }
    void redo() override { m_canvas->restoreObjects(m_objects); }

private:
    EditorCanvas* m_canvas;
    CaptureToolObjects m_objects;
    CaptureToolObjects m_backup;
};

} // namespace

EditorCanvas::EditorCanvas(const QPixmap& image, QWidget* parent)
  : QWidget(parent)
  , m_original(image)
  , m_rendered(image)
{
    setMouseTracking(true);
    setFocusPolicy(Qt::StrongFocus);
    m_undoStack.setUndoLimit(ConfigHandler().undoLimit());

    // QPixmap::copy() drops the device pixel ratio, and CaptureContext::
    // selectedScreenshotArea() is a copy() of the region the user dragged.
    // What arrives here is therefore a physical-pixel image claiming a ratio
    // of 1, so on a scaled display Qt would stretch every captured pixel
    // across 1.25 or 2 screen pixels and the capture looks soft. Restoring
    // the ratio of the screen it was taken on puts it back to one captured
    // pixel per physical screen pixel, and makes annotations here the same
    // visual weight they are on the capture overlay.
    if (qFuzzyCompare(m_original.devicePixelRatio(), qreal(1.0))) {
        QScreen* screen = QGuiAppCurrentScreen().currentScreen();
        const qreal ratio = screen ? screen->devicePixelRatio() : 1.0;
        if (ratio > 1.0) {
            m_original.setDevicePixelRatio(ratio);
            m_rendered.setDevicePixelRatio(ratio);
        }
    }

    // Widget coordinates are tool coordinates. Sizing to the image's
    // device-independent size keeps that true on high-DPI captures, where the
    // pixmap is larger than the space it occupies on screen.
    setFixedSize(m_original.deviceIndependentSize().toSize());

    m_context.screenshot = m_rendered;
    m_context.origScreenshot = m_original;
    m_context.selection = QRect(QPoint(0, 0), m_original.size());
    m_context.color = ConfigHandler().drawColor();
    m_context.toolSize = ConfigHandler().drawThickness();
    m_context.circleCount = 1;
    m_context.fullscreen = false;

    m_colorPicker = new ColorPicker(this);
    m_colorPicker->hide();
    connect(m_colorPicker,
            &ColorPicker::colorSelected,
            this,
            [this](const QColor& color) {
                m_context.mousePos = mapFromGlobal(QCursor::pos());
                setDrawColor(color);
            });
}

void EditorCanvas::markSaved()
{
    m_undoStack.setClean();
}

void EditorCanvas::setActiveToolType(CaptureTool::Type type)
{
    if (type == m_activeToolType) {
        return;
    }
    commitActiveTool();
    releaseActiveTool();

    delete m_toolPrototype;
    m_toolPrototype = nullptr;
    m_activeToolType = type;

    if (type != CaptureTool::NONE) {
        m_toolPrototype = ToolFactory().CreateTool(type, this);
        // Each tool type remembers its own thickness, the same as in the
        // capture editor
        m_context.toolSize = ConfigHandler().toolSize(type);
        // Picking a drawing tool cancels an object selection, otherwise the
        // next click would be ambiguous
        m_selectedIndex = -1;
        renderObjects();
    }
    updateCursor();
}

void EditorCanvas::setDrawColor(const QColor& color)
{
    if (!color.isValid()) {
        return;
    }
    m_context.color = color;
    ConfigHandler().setDrawColor(color);
    emit drawColorChanged(color);

    if (m_activeTool) {
        m_activeTool->onColorChanged(color);
    }
    auto selected = selectedObject();
    if (selected) {
        m_objectsBackup = m_objects;
        selected->onColorChanged(color);
        // Only a property changed, so the same index still names the same
        // object; restoring it keeps the object selected for a second edit
        const int reselect = m_selectedIndex;
        pushObjectsStateToUndoStack();
        m_selectedIndex = reselect;
        renderObjects();
    }
}

void EditorCanvas::setToolSize(int size)
{
    m_context.toolSize = size;
    if (m_activeToolType != CaptureTool::NONE) {
        ConfigHandler().setToolSize(m_activeToolType, size);
    }
    if (m_toolPrototype) {
        m_toolPrototype->onSizeChanged(size);
    }
    if (m_activeTool) {
        m_activeTool->onSizeChanged(size);
    }
    auto selected = selectedObject();
    if (selected) {
        m_objectsBackup = m_objects;
        selected->onSizeChanged(size);
        // See setDrawColor: keep the object selected across the undo push
        const int reselect = m_selectedIndex;
        pushObjectsStateToUndoStack();
        m_selectedIndex = reselect;
        renderObjects();
    }
}

void EditorCanvas::restoreObjects(const CaptureToolObjects& objects)
{
    m_objects = objects;
    // The restored list is a different set of objects, so any index into the
    // old one is meaningless
    m_selectedIndex = -1;
    restoreCircleCountState();
    renderObjects();
}

void EditorCanvas::deleteSelectedObject()
{
    if (m_selectedIndex < 0) {
        return;
    }
    m_objectsBackup = m_objects;
    m_objects.removeAt(m_selectedIndex);
    m_selectedIndex = -1;
    pushObjectsStateToUndoStack();
    renderObjects();
}

void EditorCanvas::commitActiveTool()
{
    if (!m_activeTool) {
        return;
    }
    if (m_activeTool->isValid() && !m_activeTool->editMode() && m_toolWidget) {
        pushActiveToolToStack();
    } else {
        releaseActiveTool();
        renderObjects();
    }
}

void EditorCanvas::paintEvent(QPaintEvent* event)
{
    Q_UNUSED(event)
    QPainter painter(this);
    if (!painter.isActive()) {
        return;
    }
    painter.drawPixmap(0, 0, m_rendered);

    // The in-progress object is drawn on top rather than baked in, so an
    // abandoned drag leaves no trace
    if (m_activeTool && m_mousePressed) {
        painter.setRenderHint(QPainter::Antialiasing);
        m_activeTool->process(painter, m_rendered);
    }
}

void EditorCanvas::mousePressEvent(QMouseEvent* event)
{
    setFocus();
    m_moveStarted = false;
    m_moveStartPos = QPoint();
    m_moveGrabOffset = QPoint();
    m_context.mousePos = event->pos();

    // While the picker is up it owns the mouse; the click that dismisses it
    // must not also start drawing underneath it
    if (m_colorPicker->isVisible()) {
        return;
    }
    if (event->button() == Qt::RightButton) {
        // A tool being edited owns the right button for its own purposes
        if (m_activeTool && m_activeTool->editMode()) {
            return;
        }
        showColorPicker(event->pos());
        return;
    }
    if (event->button() != Qt::LeftButton) {
        return;
    }
    m_mousePressed = true;

    // A click outside an open text box commits it before anything else
    if (m_toolWidget && !m_toolWidget->geometry().contains(event->pos())) {
        commitActiveTool();
    }

    if (startDrawing(event->pos())) {
        return;
    }
    selectObjectAt(event->pos());
    updateCursor();
}

void EditorCanvas::mouseDoubleClickEvent(QMouseEvent* event)
{
    Q_UNUSED(event)
    // Re-open the editor of an existing text object, as in the capture editor
    auto selected = selectedObject();
    if (!selected || selected->type() != CaptureTool::TYPE_TEXT) {
        return;
    }
    m_activeTool = selected;
    m_mousePressed = false;
    m_objectsBackup = m_objects;
    m_context.mousePos = *m_activeTool->pos();
    m_activeTool->setEditMode(true);
    renderObjects();
    handleToolSignal(CaptureTool::REQ_ADD_CHILD_WIDGET);
}

void EditorCanvas::mouseMoveEvent(QMouseEvent* event)
{
    m_context.mousePos = event->pos();
    if (!(event->buttons() & Qt::LeftButton)) {
        return;
    }

    if (m_activeTool && m_mousePressed && !m_activeTool->editMode()) {
        m_activeTool->drawMove(event->pos());
        update();
        return;
    }

    if (m_selectedIndex >= 0) {
        auto object = m_objects.at(m_selectedIndex);
        if (object.isNull()) {
            return;
        }
        if (!m_moveStarted) {
            if (m_moveStartPos.isNull()) {
                m_moveStartPos = event->pos();
            }
            if ((event->pos() - m_moveStartPos).manhattanLength() >
                MOUSE_DISTANCE_TO_START_MOVING) {
                m_moveStarted = true;
                m_moveGrabOffset = event->pos() - *object->pos();
                // Snapshot before the first pixel of movement so undo returns
                // to where the object started, not to an intermediate frame
                m_objectsBackup = m_objects;
                m_movingObject = true;
                setCursor(Qt::ClosedHandCursor);
            }
        }
        if (m_moveStarted) {
            object->move(event->pos() - m_moveGrabOffset);
            renderObjects();
        }
    }
}

void EditorCanvas::mouseReleaseEvent(QMouseEvent* event)
{
    if (event->button() != Qt::LeftButton) {
        return;
    }

    // Same commit gesture as the capture overlay: right click opens the
    // picker, then a left click on a swatch applies it
    if (m_colorPicker->isVisible()) {
        m_colorPicker->setNewColor();
        m_colorPicker->hide();
        updateCursor();
        return;
    }

    if (m_activeTool && m_mousePressed && !m_activeTool->editMode()) {
        m_activeTool->drawEnd(m_context.mousePos);
        if (m_activeTool->isValid()) {
            pushActiveToolToStack();
        } else if (!m_toolWidget) {
            // Tools with an editor widget, like text, stay alive until the
            // widget reports back
            releaseActiveTool();
            update();
        }
    } else if (m_movingObject) {
        pushObjectsStateToUndoStack();
        renderObjects();
    }

    m_mousePressed = false;
    m_movingObject = false;
    m_moveStarted = false;
    updateCursor();
}

void EditorCanvas::keyPressEvent(QKeyEvent* event)
{
    if (event->key() == Qt::Key_Delete || event->key() == Qt::Key_Backspace) {
        deleteSelectedObject();
        event->accept();
        return;
    }
    if (event->key() == Qt::Key_Escape && m_selectedIndex >= 0) {
        m_selectedIndex = -1;
        renderObjects();
        event->accept();
        return;
    }
    QWidget::keyPressEvent(event);
}

void EditorCanvas::showColorPicker(const QPoint& pos)
{
    // Right clicking an object recolours that object; right clicking empty
    // space sets the colour the next one will be drawn in. Selecting here is
    // what makes the first case work.
    auto selected = selectedObject();
    if (!selected || !selected->boundingRect().contains(pos)) {
        selectObjectAt(pos);
    }

    // The picker is a child widget, so it is clipped by the canvas. Unlike
    // the fullscreen overlay this canvas can be smaller than the picker or
    // scrolled, so the position has to be pulled back inside.
    QPoint topLeft(pos.x() - m_colorPicker->width() / 2,
                   pos.y() - m_colorPicker->height() / 2);
    topLeft.setX(
      qBound(0, topLeft.x(), qMax(0, width() - m_colorPicker->width())));
    topLeft.setY(
      qBound(0, topLeft.y(), qMax(0, height() - m_colorPicker->height())));
    m_colorPicker->move(topLeft);
    m_colorPicker->raise();
    m_colorPicker->show();
}

bool EditorCanvas::startDrawing(const QPoint& pos)
{
    if (!m_toolPrototype || m_activeToolType == CaptureTool::NONE) {
        return false;
    }
    // A tool that is still open, such as an uncommitted text box, must finish
    // before another object starts
    if (m_activeTool) {
        commitActiveTool();
        return false;
    }

    m_activeTool = m_toolPrototype->copy(this);
    connect(m_activeTool,
            &CaptureTool::requestAction,
            this,
            &EditorCanvas::handleToolSignal);

    m_context.mousePos = pos;
    m_activeTool->onColorChanged(m_context.color);
    m_activeTool->onSizeChanged(m_context.toolSize);
    m_activeTool->drawStart(m_context);
    return true;
}

void EditorCanvas::pushActiveToolToStack()
{
    if (!m_activeTool) {
        return;
    }
    if (m_activeTool->editMode()) {
        // The object is already in the list and was mutated in place;
        // releaseActiveTool is what records that as an undo step
        releaseActiveTool();
        renderObjects();
        return;
    }

    m_objectsBackup = m_objects;
    // append() stores a copy, so the working instance is finished with
    m_objects.append(m_activeTool);
    releaseActiveTool();
    pushObjectsStateToUndoStack();
    renderObjects();
}

void EditorCanvas::pushObjectsStateToUndoStack()
{
    m_undoStack.push(new CanvasCommand(this, m_objects, m_objectsBackup));
    m_objectsBackup.clear();
}

void EditorCanvas::releaseActiveTool()
{
    // Pushing has to wait until m_activeTool is cleared: the undo command
    // immediately replaces the object list with a deep copy, which would
    // leave any pointer into the old list dangling.
    bool recordEdit = false;
    if (m_activeTool) {
        if (m_activeTool->editMode()) {
            // The object belongs to the list already; only drop our pointer
            m_activeTool->setEditMode(false);
            recordEdit = m_activeTool->isChanged();
        } else if (!m_objects.captureToolObjects().contains(m_activeTool)) {
            delete m_activeTool;
        }
        m_activeTool = nullptr;
    }
    if (m_toolWidget) {
        // Deleted synchronously, not queued: the tool owns a QPointer to this
        // widget and clears it in its destructor, so a pending deleteLater
        // would be a double delete. Every path here is reached from a queued
        // connection, never from inside the widget's own event handler.
        m_toolWidget->hide();
        delete m_toolWidget;
        m_toolWidget = nullptr;
    }
    if (recordEdit) {
        pushObjectsStateToUndoStack();
    }
}

void EditorCanvas::renderObjects()
{
    QPixmap pixmap = m_original;
    for (const auto& object : m_objects.captureToolObjects()) {
        if (object.isNull()) {
            continue;
        }
        QPainter painter(&pixmap);
        painter.setRenderHint(QPainter::Antialiasing);
        object->process(painter, pixmap);
    }

    // The selection outline is drawn into the working copy, never into what
    // gets saved or copied
    m_rendered = pixmap;
    m_context.screenshot = pixmap;

    auto selected = selectedObject();
    if (selected && !selected->editMode()) {
        QPixmap decorated = pixmap;
        QPainter painter(&decorated);
        selected->drawObjectSelection(painter);
        m_rendered = decorated;
    }

    update();
    emit contentChanged();
}

void EditorCanvas::selectObjectAt(const QPoint& pos)
{
    if (m_activeToolType != CaptureTool::NONE) {
        return;
    }
    const int previous = m_selectedIndex;
    m_selectedIndex = m_objects.find(pos, size());
    if (m_selectedIndex != previous) {
        auto selected = selectedObject();
        if (selected && selected->size() > 0) {
            m_context.toolSize = selected->size();
        }
        renderObjects();
    }
}

void EditorCanvas::updateCursor()
{
    if (m_activeToolType != CaptureTool::NONE) {
        setCursor(Qt::CrossCursor);
    } else if (m_selectedIndex >= 0) {
        setCursor(Qt::OpenHandCursor);
    } else {
        setCursor(Qt::ArrowCursor);
    }
}

void EditorCanvas::restoreCircleCountState()
{
    int largest = 0;
    for (int i = 0; i < m_objects.size(); ++i) {
        auto object = m_objects.at(i);
        if (object.isNull() || object->type() != CaptureTool::TYPE_CIRCLECOUNT) {
            continue;
        }
        largest = qMax(largest, object->count());
    }
    m_context.circleCount = largest + 1;
}

QPointer<CaptureTool> EditorCanvas::selectedObject()
{
    return m_objects.at(m_selectedIndex);
}

void EditorCanvas::handleToolSignal(CaptureTool::Request request)
{
    switch (request) {
        case CaptureTool::REQ_ADD_CHILD_WIDGET: {
            if (!m_activeTool) {
                break;
            }
            if (m_toolWidget) {
                m_toolWidget->hide();
                delete m_toolWidget;
            }
            m_toolWidget = m_activeTool->widget();
            if (m_toolWidget) {
                m_toolWidget->setParent(this);
                m_toolWidget->move(m_context.mousePos);
                m_toolWidget->show();
                m_toolWidget->setFocus();
            }
            break;
        }
        case CaptureTool::REQ_COMMIT_CURRENT_TOOL:
            commitActiveTool();
            break;
        case CaptureTool::REQ_UNDO_MODIFICATION:
            m_undoStack.undo();
            break;
        case CaptureTool::REQ_REDO_MODIFICATION:
            m_undoStack.redo();
            break;
        case CaptureTool::REQ_INCREASE_TOOL_SIZE:
            setToolSize(m_context.toolSize + 1);
            break;
        case CaptureTool::REQ_DECREASE_TOOL_SIZE:
            setToolSize(qMax(1, m_context.toolSize - 1));
            break;
        default:
            // The editor has no capture to close, hide or export, so the
            // remaining requests have no meaning here
            break;
    }
}
