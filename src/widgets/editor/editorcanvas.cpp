// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/editor/editorcanvas.h"

#include "core/qguiappcurrentscreen.h"
#include "tools/toolfactory.h"
#include "utils/canvasgeometry.h"
#include "utils/confighandler.h"
#include "utils/toolsizewheel.h"
#include "widgets/capture/colorpicker.h"

#include <QDateTime>
#include <QGuiApplication>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QScopedPointer>
#include <QScreen>
#include <QUndoCommand>
#include <QWheelEvent>

#include <cmath>
#include <iterator>

// Matches CaptureWidget: an object only starts moving once the drag clears
// this many pixels, so a click that selects does not also nudge
#define MOUSE_DISTANCE_TO_START_MOVING 3

namespace {

// Shapes drawn by dragging. A click on its own, with the hand's usual jitter,
// would otherwise leave a speck of a shape behind. The number bubble, the
// pencil and text are meant to be placed with a single click.
bool needsDrag(CaptureTool::Type type)
{
    switch (type) {
        case CaptureTool::TYPE_RECTANGLE:
        case CaptureTool::TYPE_CIRCLE:
        case CaptureTool::TYPE_SHAPE:
        case CaptureTool::TYPE_SELECTION:
        case CaptureTool::TYPE_DRAWER:
        case CaptureTool::TYPE_ARROW:
        case CaptureTool::TYPE_MARKER:
        case CaptureTool::TYPE_PIXELATE:
        case CaptureTool::TYPE_INVERT:
            return true;
        default:
            return false;
    }
}

// Blur and invert change the pixels under them rather than drawing their own.
// On the workspace there is nothing under them past the image, so they act on
// the image alone.
bool readsPixels(CaptureTool::Type type)
{
    return type == CaptureTool::TYPE_PIXELATE ||
           type == CaptureTool::TYPE_INVERT;
}

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

    // The image floats in the middle of a workspace with room on every side.
    // Sizing from the device-independent size keeps one image pixel one tool
    // pixel on high-DPI captures, where the pixmap is larger than the space it
    // occupies. The margin is a whole number of physical pixels, so the image
    // is placed on the pixel grid and copied rather than resampled.
    const qreal ratio = m_original.devicePixelRatio();
    const QSize imageSize = m_original.deviceIndependentSize().toSize();
    const int margin = CanvasGeometry::workspaceMargin(imageSize, ratio);
    const int physicalMargin = qRound(margin * ratio);
    m_imageRect = QRect(QPoint(margin, margin), imageSize);
    m_imagePhysical =
      QRect(QPoint(physicalMargin, physicalMargin), m_original.size());
    m_base = QPixmap(m_original.size() +
                     QSize(physicalMargin * 2, physicalMargin * 2));
    m_base.setDevicePixelRatio(ratio);
    m_base.fill(Qt::transparent);
    {
        QPainter painter(&m_base);
        painter.drawPixmap(m_imageRect.topLeft(), m_original);
    }
    m_canvasRect = QRect(QPoint(0, 0), m_base.deviceIndependentSize().toSize());
    m_rendered = m_base;
    m_exported = m_original;
    updateCanvasSize();

    m_context.screenshot = m_base;
    m_context.origScreenshot = m_base;
    m_context.selection = QRect(QPoint(0, 0), m_base.size());
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
    size = qBound(MinToolSize, size, MaxToolSize);
    if (m_context.toolSize == size) {
        return;
    }
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
    emit toolSizeChanged(size);
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

void EditorCanvas::adoptObjects(const QList<QPointer<CaptureTool>>& objects,
                                const QPoint& offset)
{
    for (const auto& object : objects) {
        if (object.isNull()) {
            continue;
        }
        // The overlay's objects die with the overlay, so each one is copied
        // under this canvas before the overlay finishes tearing down.
        // append() copies again under the same parent, so the intermediate
        // copy is only a place to apply the offset.
        QScopedPointer<CaptureTool> local(object->copy(this));
        local->setEditMode(false);
        local->translate(offset + m_imageRect.topLeft());
        m_objects.append(local.data());
    }
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

QPoint EditorCanvas::toImage(const QPoint& widgetPos) const
{
    return m_canvasRect.topLeft() + QPoint(qRound(widgetPos.x() / m_zoom),
                                           qRound(widgetPos.y() / m_zoom));
}

QPoint EditorCanvas::fromImage(const QPoint& imagePos) const
{
    const QPoint local = imagePos - m_canvasRect.topLeft();
    return { qRound(local.x() * m_zoom), qRound(local.y() * m_zoom) };
}

void EditorCanvas::updateCanvasSize()
{
    setFixedSize(qMax(1, qRound(m_canvasRect.width() * m_zoom)),
                 qMax(1, qRound(m_canvasRect.height() * m_zoom)));
}

void EditorCanvas::setZoom(qreal zoom)
{
    const qreal clamped =
      qBound(CanvasGeometry::MinZoom, zoom, CanvasGeometry::MaxZoom);
    if (qFuzzyCompare(clamped, m_zoom)) {
        return;
    }
    m_zoom = clamped;
    updateCanvasSize();
    // An open text box is sized in widget pixels, so it has to be told the new
    // scale or it keeps rendering at the old one
    if (m_activeTool) {
        m_activeTool->setEditorScale(m_zoom);
        if (m_toolWidget) {
            const QPoint* toolPos = m_activeTool->pos();
            if (toolPos) {
                m_toolWidget->move(fromImage(*toolPos) +
                                   m_activeTool->childWidgetOffset());
            }
        }
    }
    update();
    emit zoomChanged(m_zoom);
}

bool EditorCanvas::zoomBy(int notches, const QPoint& anchor)
{
    const qreal next = CanvasGeometry::zoomStep(m_zoom, notches);
    if (qFuzzyCompare(next, m_zoom)) {
        return false;
    }
    // Remember what the pointer is over before the scale changes, so the
    // caller can scroll it back under the pointer afterwards
    const QPoint held = toImage(anchor);
    setZoom(next);
    m_lastZoomAnchor = held;
    return true;
}

void EditorCanvas::setGridVisible(bool visible)
{
    if (m_gridVisible == visible) {
        return;
    }
    m_gridVisible = visible;
    update();
}

void EditorCanvas::paintGrid(QPainter& painter, const QRect& dirty) const
{
    // The step is in image pixels so the lines stay attached to the image
    // while zooming, but it widens as the zoom drops so the grid never turns
    // into a solid wash
    static constexpr int Steps[] = { 10, 20, 50, 100, 200, 500, 1000 };
    static constexpr qreal MinSpacing = 12.0;
    int step = Steps[std::size(Steps) - 1];
    for (int candidate : Steps) {
        if (candidate * m_zoom >= MinSpacing) {
            step = candidate;
            break;
        }
    }
    const int major = step * 5;

    // Mid grey reads on both light and dark captures; every fifth line is
    // stronger so distances can be counted at a glance
    const QPen minorPen(QColor(128, 128, 128, 90), 0);
    const QPen majorPen(QColor(128, 128, 128, 170), 0);

    // Drawn in widget pixels, after the image, so lines are one screen pixel
    // wide at any zoom instead of scaling with it
    const QRectF area = QRectF(rect()).intersected(QRectF(dirty));
    auto toWidget = [this](int imageCoord, int origin) {
        return (imageCoord - origin) * m_zoom;
    };
    auto firstLine = [step](int from) {
        return static_cast<int>(std::floor(from / double(step))) * step;
    };

    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, false);
    // Counted from the image's corner, not the workspace's, so the lines
    // measure the image
    const int left = m_canvasRect.left() - m_imageRect.left();
    const int top = m_canvasRect.top() - m_imageRect.top();
    const int imageFromX = left + static_cast<int>(area.left() / m_zoom);
    const int imageToX = left + static_cast<int>(area.right() / m_zoom) + 1;
    for (int x = firstLine(imageFromX); x <= imageToX; x += step) {
        painter.setPen(x % major == 0 ? majorPen : minorPen);
        const qreal wx = toWidget(x, left);
        painter.drawLine(QPointF(wx, area.top()), QPointF(wx, area.bottom()));
    }
    const int imageFromY = top + static_cast<int>(area.top() / m_zoom);
    const int imageToY = top + static_cast<int>(area.bottom() / m_zoom) + 1;
    for (int y = firstLine(imageFromY); y <= imageToY; y += step) {
        painter.setPen(y % major == 0 ? majorPen : minorPen);
        const qreal wy = toWidget(y, top);
        painter.drawLine(QPointF(area.left(), wy), QPointF(area.right(), wy));
    }
    painter.restore();
}

void EditorCanvas::paintEvent(QPaintEvent* event)
{
    QPainter painter(this);
    if (!painter.isActive()) {
        return;
    }

    // One transform for the whole canvas: everything below draws in image
    // coordinates and knows nothing about the zoom or the canvas origin.
    painter.scale(m_zoom, m_zoom);
    painter.translate(-m_canvasRect.topLeft());
    if (m_zoom < 1.0) {
        // Downscaling without this is visibly aliased on text-heavy captures
        painter.setRenderHint(QPainter::SmoothPixmapTransform);
    }

    paintWorkspace(painter);
    painter.drawPixmap(0, 0, m_rendered);

    // The in-progress object is drawn on top rather than baked in, so an
    // abandoned drag leaves no trace
    if (m_activeTool && m_mousePressed) {
        painter.save();
        painter.setRenderHint(QPainter::Antialiasing);
        if (readsPixels(m_activeTool->type())) {
            painter.setClipRect(m_imageRect);
        }
        m_activeTool->process(painter, m_rendered);
        painter.restore();
    }

    if (m_gridVisible) {
        painter.resetTransform();
        paintGrid(painter, event->rect());
    }
}

void EditorCanvas::mousePressEvent(QMouseEvent* event)
{
    setFocus();
    m_moveStarted = false;
    m_moveStartPos = QPoint();
    m_moveGrabOffset = QPoint();
    m_context.mousePos = toImage(event->pos());

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
    m_pressPos = event->pos();

    // A click outside an open text box commits it before anything else
    if (m_toolWidget && !m_toolWidget->geometry().contains(event->pos())) {
        commitActiveTool();
    }

    if (startDrawing(toImage(event->pos()))) {
        return;
    }
    selectObjectAt(toImage(event->pos()));
    updateCursor();
    if (m_activeToolType == CaptureTool::NONE) {
        // Grabbing either way: the annotation under the press, or with
        // nothing there, the view itself, as a hand tool does in a viewer
        m_panning = m_selectedIndex < 0;
        m_panLast = event->globalPosition().toPoint();
        setCursor(Qt::ClosedHandCursor);
    }
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

void EditorCanvas::wheelEvent(QWheelEvent* event)
{
    // Windows delivers the wheel to whatever window is under the pointer,
    // active or not. An editor in the background must not change zoom or
    // tool size because the pointer drifted over it.
    if (window() && !window()->isActiveWindow()) {
        event->ignore();
        return;
    }

    // With no tool picked the wheel zooms at the pointer, the way image
    // viewers do. Ctrl+wheel zooms from any tool, so the user never has to
    // leave the pencil to get closer; Zoom mode gives the plain wheel the
    // same job. A sideways-only scroll still pans.
    const bool ctrlHeld = event->modifiers().testFlag(Qt::ControlModifier);
    const bool noTool = m_activeToolType == CaptureTool::NONE;
    if ((ctrlHeld || m_zoomMode || noTool) && event->angleDelta().y() != 0) {
        if (event->angleDelta().y() != 0) {
            const int notches = event->angleDelta().y() > 0 ? 1 : -1;
            emit zoomRequested(notches, event->position().toPoint());
        }
        event->accept();
        return;
    }

    if (noTool) {
        event->ignore();
        return;
    }
    event->accept();

    const ToolSizeWheel::Step step =
      ToolSizeWheel::evaluate(event->angleDelta().y(),
                              QDateTime::currentMSecsSinceEpoch(),
                              m_lastWheelMs);
    if (!step.accepted) {
        return;
    }

    // Same order as the capture overlay: a tool that wants the wheel for
    // itself, as the circle counter does while Ctrl is held, gets it first
    const bool adjustmentHeld =
      QGuiApplication::keyboardModifiers().testFlag(Qt::ControlModifier);
    if (step.fromWheel && m_toolPrototype &&
        m_toolPrototype->handleMouseWheelEvent(
          step.delta, adjustmentHeld, m_context)) {
        update();
        return;
    }

    setToolSize(m_context.toolSize + step.delta);
}

void EditorCanvas::mouseMoveEvent(QMouseEvent* event)
{
    m_context.mousePos = toImage(event->pos());
    if (!(event->buttons() & Qt::LeftButton)) {
        return;
    }

    if (m_panning) {
        // Screen positions, not widget ones: the widget itself moves as the
        // view scrolls, which would feed straight back into the delta
        const QPoint now = event->globalPosition().toPoint();
        emit panRequested(now - m_panLast);
        m_panLast = now;
        return;
    }

    if (m_activeTool && m_mousePressed && !m_activeTool->editMode()) {
        m_activeTool->drawMove(m_context.mousePos);
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
            // The threshold is a gesture distance on screen, so it stays in
            // widget pixels; at low zoom an image-pixel threshold would make
            // objects impossible to nudge
            if ((event->pos() - m_moveStartPos).manhattanLength() >
                MOUSE_DISTANCE_TO_START_MOVING) {
                m_moveStarted = true;
                m_moveGrabOffset = m_context.mousePos - *object->pos();
                // Snapshot before the first pixel of movement so undo returns
                // to where the object started, not to an intermediate frame
                m_objectsBackup = m_objects;
                m_movingObject = true;
                setCursor(Qt::ClosedHandCursor);
            }
        }
        if (m_moveStarted) {
            object->move(m_context.mousePos - m_moveGrabOffset);
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
        // The picker can open mid-drag with the right button, and this
        // release is that drag's end too
        m_panning = false;
        m_mousePressed = false;
        updateCursor();
        return;
    }

    if (m_panning) {
        m_panning = false;
        m_mousePressed = false;
        updateCursor();
        return;
    }

    if (m_activeTool && m_mousePressed && !m_activeTool->editMode()) {
        m_activeTool->drawEnd(m_context.mousePos);
        // Measured on screen, like the move threshold, so it means the same
        // at any zoom
        const bool clickOnly = (event->pos() - m_pressPos).manhattanLength() <=
                               MOUSE_DISTANCE_TO_START_MOVING;
        if (clickOnly && needsDrag(m_activeTool->type())) {
            releaseActiveTool();
            update();
        } else if (m_activeTool->isValid()) {
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

QColor EditorCanvas::workspaceColor() const
{
    // The theme's surface colour: white in light mode, near-black in dark
    return palette().color(QPalette::Base);
}

void EditorCanvas::changeEvent(QEvent* event)
{
    QWidget::changeEvent(event);
    // Windows switched theme: the workspace colour changed, and with it what
    // an export puts behind annotations past the image
    if (event->type() == QEvent::PaletteChange) {
        renderObjects();
    }
}

void EditorCanvas::keyPressEvent(QKeyEvent* event)
{
    if (event->key() == Qt::Key_Delete || event->key() == Qt::Key_Backspace) {
        deleteSelectedObject();
        event->accept();
        return;
    }
    // Escape steps back one level: out of the picked tool first, and only
    // then out of the object selection
    if (event->key() == Qt::Key_Escape &&
        (m_activeToolType != CaptureTool::NONE || m_zoomMode)) {
        // An open text box keeps what was typed rather than losing it
        commitActiveTool();
        emit selectModeRequested();
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
    // The argument is a widget position because the picker is a child widget,
    // but object picking happens in image space
    const QPoint imagePos = toImage(pos);
    auto selected = selectedObject();
    if (!selected || !selected->boundingRect().contains(imagePos)) {
        selectObjectAt(imagePos);
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

void EditorCanvas::paintWorkspace(QPainter& painter) const
{
    // Drawn under the image and the annotations, so it shows only where
    // nothing has been drawn. The scroll area's backdrop around it marks
    // where the drawable area ends.
    painter.save();
    painter.fillRect(m_canvasRect, workspaceColor());
    // A soft shadow lifts the image off the workspace; a few widening rings
    // are enough and cost nothing next to the pixmap itself
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(Qt::NoPen);
    const bool darkSurface = workspaceColor().lightness() < 128;
    for (int ring = 1; ring <= 6; ++ring) {
        const QColor shade(
          0, 0, 0, darkSurface ? 30 - ring * 4 : 18 - ring * 2);
        painter.setBrush(shade);
        painter.drawRoundedRect(
          QRectF(m_imageRect).adjusted(-ring, -ring + 2, ring, ring + 2),
          ring,
          ring);
    }
    painter.restore();
}

void EditorCanvas::processOnImage(const QPointer<CaptureTool>& object,
                                  QPixmap& workspace)
{
    // Run on the image cut out of the workspace, with the object moved into
    // image coordinates: the pixmap's corner is then the tool's (0,0) again,
    // which is what these tools assume when they index pixels. Sampling the
    // transparent workspace instead would darken the image's edge.
    QPixmap image = workspace.copy(m_imagePhysical);
    image.setDevicePixelRatio(workspace.devicePixelRatio());
    QScopedPointer<CaptureTool> local(object->copy(nullptr));
    local->translate(-m_imageRect.topLeft());
    {
        QPainter painter(&image);
        painter.setRenderHint(QPainter::Antialiasing);
        local->process(painter, image);
    }
    QPainter painter(&workspace);
    painter.setCompositionMode(QPainter::CompositionMode_Source);
    painter.drawPixmap(m_imageRect.topLeft(), image);
}

QPixmap EditorCanvas::exportFrom(const QPixmap& flattened)
{
    const qreal ratio = flattened.devicePixelRatio();
    QVector<QRect> reach;
    for (const auto& object : m_objects.captureToolObjects()) {
        if (object.isNull() || readsPixels(object->type())) {
            continue;
        }
        // A freehand stroke's bounds are its points, not its ink
        const int pad = qMax(2, object->size() / 2 + 2);
        reach.append(CanvasGeometry::toPhysical(
          object->boundingRect().adjusted(-pad, -pad, pad, pad), ratio));
    }
    const QRect area =
      CanvasGeometry::exportRect(m_imagePhysical, reach, flattened.rect());
    QPixmap cropped = flattened.copy(area);
    cropped.setDevicePixelRatio(ratio);
    if (area == m_imagePhysical) {
        return cropped;
    }
    // Past the image the workspace is transparent. The export fills it with
    // the colour the workspace shows, so what is saved is what was on screen;
    // transparency would turn black wherever the alpha is dropped, which is
    // most places a capture is pasted.
    QPixmap backed(cropped.size());
    backed.setDevicePixelRatio(ratio);
    backed.fill(workspaceColor());
    QPainter painter(&backed);
    painter.drawPixmap(0, 0, cropped);
    return backed;
}

void EditorCanvas::renderObjects()
{
    QPixmap pixmap = m_base;
    for (const auto& object : m_objects.captureToolObjects()) {
        if (object.isNull()) {
            continue;
        }
        if (readsPixels(object->type())) {
            processOnImage(object, pixmap);
            continue;
        }
        QPainter painter(&pixmap);
        painter.setRenderHint(QPainter::Antialiasing);
        object->process(painter, pixmap);
    }

    // The selection outline is drawn into the working copy, never into what
    // gets saved or copied
    m_rendered = pixmap;
    m_exported = exportFrom(pixmap);
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
    // Picking renders the objects into a scratch pixmap and reads a pixel, so
    // it works in image space like the objects themselves, not in the scaled
    // widget space size() reports
    m_selectedIndex = m_objects.find(pos, m_canvasRect.size());
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
    if (m_activeToolType == CaptureTool::TYPE_TEXT) {
        // The text tool places an insertion point, not a region, and the
        // I-beam is what tells the user the line will straddle the cursor.
        setCursor(Qt::IBeamCursor);
    } else if (m_activeToolType != CaptureTool::NONE) {
        setCursor(Qt::CrossCursor);
    } else {
        // With no tool picked a press grabs: the annotation under it, or the
        // view when there is none. The pointer says so before the press.
        setCursor(Qt::OpenHandCursor);
    }
}

void EditorCanvas::restoreCircleCountState()
{
    int largest = 0;
    for (int i = 0; i < m_objects.size(); ++i) {
        auto object = m_objects.at(i);
        if (object.isNull() ||
            object->type() != CaptureTool::TYPE_CIRCLECOUNT) {
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
            m_activeTool->setEditorScale(m_zoom);
            m_toolWidget = m_activeTool->widget();
            if (m_toolWidget) {
                m_toolWidget->setParent(this);
                // A tool that carries its own position anchors the editor
                // from there, so re-opening an existing object lands the
                // editor exactly over it; the offset maps the object's corner
                // onto the widget's corner.
                const QPoint* toolPos = m_activeTool->pos();
                m_toolWidget->move(
                  fromImage(toolPos ? *toolPos : m_context.mousePos) +
                  m_activeTool->childWidgetOffset());
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
