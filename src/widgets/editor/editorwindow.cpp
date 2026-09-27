// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/editor/editorwindow.h"
#include "widgets/editor/editortheme.h"

#include "core/flameshotdaemon.h"
#include "core/qguiappcurrentscreen.h"
#if defined(Q_OS_WIN)
#include "tools/ocr/ocrresultswindow.h"
#endif
#include "tools/shape/shapetool.h"
#include "tools/toolfactory.h"
#include "utils/abstractlogger.h"
#include "utils/colorutils.h"
#include "utils/confighandler.h"
#include "utils/filenamehandler.h"
#include "utils/globalvalues.h"
#include "utils/pathinfo.h"
#if defined(Q_OS_WIN)
#include "utils/printscreenkey.h"
#endif
#include "utils/screenshotsaver.h"
#include "widgets/editor/editorcanvas.h"
#include "widgets/editor/editorfilmstrip.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QCloseEvent>
#include <QColorDialog>
#include <QDateTime>
#include <QDir>
#include <QFileDialog>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QMenu>
#include <QMessageBox>
#include <QPainter>
#include <QPixmap>
#include <QPropertyAnimation>
#include <QPushButton>
#include <QScreen>
#include <QScrollArea>
#include <QScrollBar>
#include <QSpinBox>
#include <QStackedWidget>
#include <QStandardPaths>
#include <QStatusBar>
#include <QStringList>
#include <QStyle>
#include <QStyleHints>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QVBoxLayout>
#include <QVariant>

// CaptureTool::Type values are persisted in user configs, so the zoom
// action carries a sentinel that can never collide with one instead of
// claiming a type number of its own.
static constexpr int ZoomActionData = -100;

QPointer<EditorWindow> EditorWindow::s_instance;

namespace {

// Annotation tools only. Capture-time actions (accept, exit, pin, upload,
// region selection) have no meaning once the image is already captured.
const QVector<CaptureTool::Type>& editorToolTypes()
{
    static const QVector<CaptureTool::Type> types = {
        CaptureTool::TYPE_PENCIL,
        CaptureTool::TYPE_DRAWER,
        CaptureTool::TYPE_ARROW,
        // One button for every closed shape; the separate rectangle, circle
        // and hollow-square tools are reachable through its picker
        CaptureTool::TYPE_SHAPE,
        CaptureTool::TYPE_MARKER,
        CaptureTool::TYPE_TEXT,
        CaptureTool::TYPE_PIXELATE,
        CaptureTool::TYPE_INVERT,
        CaptureTool::TYPE_CIRCLECOUNT,
    };
    return types;
}

constexpr int ThumbnailRefreshMs = 250;

// Editor-only keys for the tools that have no configuration entry. The
// shape button is reached through the rectangle and circle keys instead, so
// it needs none of its own.
//
// Select-and-move is configured as TYPE_MOVE_OBJECT so the editor and the
// capture overlay answer to the same key; V is kept as an alias because it
// shipped as this mode's only key before that entry existed.
constexpr auto LegacySelectToolKey = Qt::Key_V;
constexpr auto CircleCountKey = Qt::Key_N;

// The name a tool's shortcut is stored under is its Q_ENUM name
QString shortcutFor(CaptureTool::Type type)
{
    return ConfigHandler().shortcut(QVariant::fromValue(type).toString());
}

// One shape key, and the variant the tool it belongs to draws in the capture
// overlay
struct ShapeBinding
{
    CaptureTool::Type key;
    ShapeTool::Kind kind;
    ShapeTool::Style style;
};

/**
 * @brief Maps the three retired shape tools onto shape-button variants.
 *
 * RectangleTool fills, SelectionTool strokes a rectangle and CircleTool
 * strokes an ellipse. Those three still exist and still hold R, S and C, so
 * the editor has to reproduce what pressing each of them draws rather than
 * inventing its own arrangement. There is no key for a filled circle or for
 * a highlight, because the overlay has none either.
 */
const QVector<ShapeBinding>& shapeBindings()
{
    static const QVector<ShapeBinding> bindings = {
        { CaptureTool::TYPE_RECTANGLE,
          ShapeTool::Kind::Square,
          ShapeTool::Style::Filled },
        { CaptureTool::TYPE_SELECTION,
          ShapeTool::Kind::Square,
          ShapeTool::Style::Hollow },
        { CaptureTool::TYPE_CIRCLE,
          ShapeTool::Kind::Circle,
          ShapeTool::Style::Hollow },
    };
    return bindings;
}

// What the tooltip should advertise, which is not always a configured key
QString editorShortcutHint(CaptureTool::Type type)
{
    switch (type) {
        case CaptureTool::TYPE_SHAPE: {
            // One button, one key per variant it absorbed
            QStringList keys;
            for (const ShapeBinding& binding : shapeBindings()) {
                const QString key = shortcutFor(binding.key);
                if (!key.isEmpty()) {
                    keys << key;
                }
            }
            return keys.join(QStringLiteral(" / "));
        }
        case CaptureTool::TYPE_CIRCLECOUNT:
            return QKeySequence(QKeyCombination(CircleCountKey)).toString();
        default:
            return shortcutFor(type);
    }
}

QString withShortcut(const QString& description, const QString& key)
{
    if (key.isEmpty()) {
        return description;
    }
    return QStringLiteral("%1 (%2)").arg(description, key);
}

} // namespace

EditorWindow::EditorWindow(QWidget* parent)
  : QMainWindow(parent)
{
    setAttribute(Qt::WA_DeleteOnClose);
    setWindowTitle(tr("Phramer Editor"));
    setWindowIcon(GlobalValues::appIcon());
    // The style sheet is scoped to this name so dialogs keep the platform look
    setObjectName(QStringLiteral("phramerEditor"));
    // Wide enough for the labelled toolbar without an overflow arrow
    resize(1240, 760);

    m_pages = new QStackedWidget(this);

    m_filmstrip = new EditorFilmstrip(this);
    m_filmstrip->setObjectName(QStringLiteral("editorFilmstrip"));
    connect(m_filmstrip,
            &EditorFilmstrip::imageActivated,
            this,
            [this](int index) { setCurrentIndex(index); });
    connect(m_filmstrip,
            &EditorFilmstrip::imagesReordered,
            this,
            &EditorWindow::onImagesReordered);

    auto* central = new QWidget(this);
    auto* layout = new QVBoxLayout(central);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    buildEmptyState();
    layout->addWidget(m_emptyState, 1);
    layout->addWidget(m_pages, 1);
    layout->addWidget(m_filmstrip);
    setCentralWidget(central);

    buildToolBar();
    buildStatusBar();
    applyTheme();
    connect(QGuiApplication::styleHints(),
            &QStyleHints::colorSchemeChanged,
            this,
            &EditorWindow::applyTheme);

    m_thumbnailTimer = new QTimer(this);
    m_thumbnailTimer->setSingleShot(true);
    m_thumbnailTimer->setInterval(ThumbnailRefreshMs);
    connect(m_thumbnailTimer, &QTimer::timeout, this, [this]() {
        EditorCanvas* canvas = currentCanvas();
        if (canvas) {
            m_filmstrip->updateImage(m_currentIndex, canvas->rendered());
        }
    });

    updateNavigationState();
    updateUndoState();
    updateColorSwatch();
    updateEmptyState();
}

EditorWindow::~EditorWindow()
{
    if (s_instance == this) {
        s_instance = nullptr;
    }
}

void EditorWindow::openEmpty()
{
    if (s_instance.isNull()) {
        s_instance = new EditorWindow();
    }
    s_instance->updateEmptyState();
    s_instance->show();
    s_instance->raise();
    s_instance->activateWindow();
}

void EditorWindow::buildEmptyState()
{
    m_emptyState = new QWidget(this);
    auto* layout = new QVBoxLayout(m_emptyState);
    layout->setSpacing(10);
    layout->addStretch(1);

    auto* icon = new QLabel(m_emptyState);
    icon->setPixmap(GlobalValues::appIcon().pixmap(72, 72));
    icon->setAlignment(Qt::AlignHCenter);
    layout->addWidget(icon);

    auto* title = new QLabel(
      tr("Perform a capture to load an image into the editor"), m_emptyState);
    QFont font = title->font();
    font.setPointSizeF(font.pointSizeF() * 1.35);
    font.setWeight(QFont::DemiBold);
    title->setFont(font);
    title->setAlignment(Qt::AlignHCenter);
    layout->addWidget(title);

    m_emptyHint = new QLabel(m_emptyState);
    m_emptyHint->setAlignment(Qt::AlignHCenter);
    m_emptyHint->setWordWrap(true);
    m_emptyHint->setTextFormat(Qt::RichText);
    layout->addWidget(m_emptyHint);
    layout->addStretch(1);
}

void EditorWindow::updateEmptyState()
{
    const bool empty = m_canvases.isEmpty();
    m_emptyState->setVisible(empty);
    m_pages->setVisible(!empty);
    m_filmstrip->setVisible(!empty);
    // Nothing on the toolbar has anything to act on without an image
    if (m_toolBar) {
        m_toolBar->setEnabled(!empty);
    }
    if (!empty) {
        return;
    }

    ConfigHandler config;
    const auto keyText = [](const QString& stored) {
        return QKeySequence(stored).toString(QKeySequence::NativeText);
    };
    const QString capture = keyText(config.shortcut("TAKE_SCREENSHOT"));
    QStringList keys;
    if (!capture.isEmpty()) {
        keys << QStringLiteral("<b>%1</b>").arg(capture.toHtmlEscaped());
    }
#if defined(Q_OS_WIN)
    // Only when Phramer has taken Print Screen over from Windows; otherwise
    // that key still opens the Windows snipping tool
    const QString printScreen =
      QKeySequence(Qt::Key_Print).toString(QKeySequence::NativeText);
    if (PrintScreenKey::isSnippingDisabled() && capture != printScreen) {
        keys << QStringLiteral("<b>%1</b>").arg(printScreen.toHtmlEscaped());
    }
#endif

    QString first = keys.isEmpty()
                      ? tr("Take a capture from the tray icon")
                      : tr("Press %1 to capture")
                          .arg(keys.join(QStringLiteral(" %1 ").arg(tr("or"))));
    QString second;
    if (config.autoOpenInEditor()) {
        second = tr("It opens here automatically.");
    } else {
        const QString editorKey =
          keyText(config.shortcut("TYPE_OPEN_IN_EDITOR"));
        second = editorKey.isEmpty()
                   ? tr("Then pick Editor on the capture toolbar.")
                   : tr("Then pick Editor (<b>%1</b>) on the capture toolbar.")
                       .arg(editorKey.toHtmlEscaped());
    }
    m_emptyHint->setText(first + QStringLiteral(". ") + second);
}

bool EditorWindow::isOpen()
{
    return !s_instance.isNull();
}

void EditorWindow::addCapture(const QPixmap& capture,
                              const QList<QPointer<CaptureTool>>& objects,
                              const QPoint& offset)
{
    if (s_instance.isNull()) {
        s_instance = new EditorWindow();
    }
    s_instance->addImage(capture, objects, offset);
    s_instance->show();
    s_instance->raise();
    s_instance->activateWindow();
}

void EditorWindow::addImage(const QPixmap& image,
                            const QList<QPointer<CaptureTool>>& objects,
                            const QPoint& offset)
{
    if (image.isNull()) {
        return;
    }

    auto* canvas = new EditorCanvas(image, this);
    canvas->setGridVisible(m_gridAction && m_gridAction->isChecked());
    if (!objects.isEmpty()) {
        canvas->adoptObjects(objects, offset);
    }
    connect(canvas,
            &EditorCanvas::contentChanged,
            this,
            &EditorWindow::onCanvasContentChanged);
    connect(
      canvas, &EditorCanvas::zoomChanged, this, &EditorWindow::onZoomChanged);
    connect(canvas,
            &EditorCanvas::zoomRequested,
            this,
            &EditorWindow::zoomCurrentCanvas);
    // The canvas has its own right-click colour picker, so the toolbar
    // swatch cannot assume it is the only thing that sets the colour
    connect(canvas, &EditorCanvas::drawColorChanged, this, [this]() {
        updateColorSwatch();
    });
    // The wheel and the size shortcuts change the size on the canvas, so the
    // field has to follow rather than be the only way to set it
    connect(
      canvas, &EditorCanvas::toolSizeChanged, this, [this, canvas](int size) {
          // The toolbar is shared, so only the visible canvas may drive it
          if (currentCanvas() != canvas) {
              return;
          }
          m_sizeBox->blockSignals(true);
          m_sizeBox->setValue(size);
          m_sizeBox->blockSignals(false);
      });
    // The canvas is fixed to the image size; the scroll area handles captures
    // larger than the window
    auto* scroll = new QScrollArea(m_pages);
    scroll->setObjectName(QStringLiteral("editorPage"));
    scroll->setFrameShape(QFrame::NoFrame);
    scroll->setAlignment(Qt::AlignCenter);
    scroll->setWidget(canvas);
    scroll->setWidgetResizable(false);
    scroll->setBackgroundRole(QPalette::Dark);

    m_canvases.append(canvas);
    m_pages->addWidget(scroll);
    // The rendered image, so a capture that arrived with annotations shows
    // them in its thumbnail
    m_filmstrip->appendImage(canvas->rendered());

    // Only for the first image, and only before the window is up: after that
    // the size is the user's business
    if (m_canvases.size() == 1 && !isVisible()) {
        resizeToFit(canvas);
    }

    setCurrentIndex(m_canvases.size() - 1);
    updateEmptyState();
}

void EditorWindow::resizeToFit(EditorCanvas* canvas)
{
    QScreen* screen = QGuiAppCurrentScreen().currentScreen();
    if (!screen) {
        screen = QGuiApplication::primaryScreen();
    }
    if (!screen) {
        return;
    }

    // Approximate, because none of this is laid out yet: the toolbar and
    // scroll frame on the sides, and the toolbar, filmstrip and status bar
    // stacked vertically. Erring large just means a little empty canvas
    // border, which is better than immediate scrollbars on an image that
    // would have fit.
    const QSize chrome(40, 210);
    QSize target = canvas->size() + chrome;
    // Leave room for the taskbar and window frame rather than filling the
    // screen edge to edge
    target = target.boundedTo(screen->availableSize() * 0.92);
    target = target.expandedTo(QSize(720, 520));
    resize(target);
}

void EditorWindow::buildToolBar()
{
    auto* bar = addToolBar(tr("Tools"));
    m_toolBar = bar;
    bar->setObjectName(QStringLiteral("editorToolBar"));
    bar->setMovable(false);
    bar->setIconSize(QSize(20, 20));
    // Names under the icons, the same one-word names the capture overlay
    // shows. applyTheme() re-picks the icons from the "phramerIcon" and
    // "phramerTool" properties set on each action below.
    bar->setToolButtonStyle(ConfigHandler().showToolLabels()
                              ? Qt::ToolButtonTextUnderIcon
                              : Qt::ToolButtonIconOnly);

    // Tool icons come in a light and a dark variant; pick the one that
    // contrasts with whatever palette the window is actually using
    const QColor background = palette().color(QPalette::Window);
    m_toolbarBackground = background;
    const QString iconDir = ColorUtils::colorIsDark(background)
                              ? PathInfo::whiteIconPath()
                              : PathInfo::blackIconPath();

    m_toolGroup = new QActionGroup(this);
    m_toolGroup->setExclusive(true);

    auto* selectAction =
      bar->addAction(QIcon(iconDir + "pan-tool.svg"), tr("Select and move"));
    // The overlay's hand tool, but in the editor it is also how objects
    // are picked, so it is called Select here
    selectAction->setIconText(
      ToolFactory::labelWithShortcut(CaptureTool::TYPE_MOVE_OBJECT,
                                     shortcutFor(CaptureTool::TYPE_MOVE_OBJECT),
                                     tr("Select")));
    selectAction->setProperty("phramerIcon", QStringLiteral("pan-tool.svg"));
    selectAction->setCheckable(true);
    selectAction->setChecked(true);
    selectAction->setData(static_cast<int>(CaptureTool::NONE));
    selectAction->setToolTip(
      withShortcut(tr("Select, move and delete objects you have placed"),
                   shortcutFor(CaptureTool::TYPE_MOVE_OBJECT)));
    m_toolGroup->addAction(selectAction);
    connect(selectAction,
            &QAction::triggered,
            this,
            &EditorWindow::onToolActionTriggered);
    m_selectAction = selectAction;

    // Called Zoom, not Magnifier: "Show magnifier" is already a shipped
    // setting for the capture overlay's selection loupe, and two different
    // features under one word makes every bug report ambiguous.
    m_zoomAction = bar->addAction(QIcon(iconDir + "magnify.svg"), tr("Zoom"));
    m_zoomAction->setIconText(
      tr("Zoom (%1)")
        .arg(QKeySequence(Qt::Key_Z).toString(QKeySequence::NativeText)));
    m_zoomAction->setProperty("phramerIcon", QStringLiteral("magnify.svg"));
    m_zoomAction->setCheckable(true);
    m_zoomAction->setData(ZoomActionData);
    m_zoomAction->setShortcut(QKeySequence(Qt::Key_Z));
    auto* zoomInAction = new QAction(tr("Zoom In"), this);
    zoomInAction->setShortcuts(
      { QKeySequence::ZoomIn, QKeySequence(Qt::CTRL | Qt::Key_Equal) });
    connect(zoomInAction, &QAction::triggered, this, [this]() {
        zoomCurrentCanvas(1, viewportCentre());
    });
    addAction(zoomInAction);

    auto* zoomOutAction = new QAction(tr("Zoom Out"), this);
    zoomOutAction->setShortcut(QKeySequence::ZoomOut);
    connect(zoomOutAction, &QAction::triggered, this, [this]() {
        zoomCurrentCanvas(-1, viewportCentre());
    });
    addAction(zoomOutAction);

    auto* zoomResetAction = new QAction(tr("Actual Size"), this);
    zoomResetAction->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_0));
    connect(zoomResetAction, &QAction::triggered, this, [this]() {
        if (EditorCanvas* canvas = currentCanvas()) {
            canvas->setZoom(1.0);
        }
    });
    addAction(zoomResetAction);

    m_zoomAction->setToolTip(
      tr("Zoom the view with the wheel. Ctrl+wheel zooms from any tool."));
    m_toolGroup->addAction(m_zoomAction);
    connect(m_zoomAction,
            &QAction::triggered,
            this,
            &EditorWindow::onToolActionTriggered);

    // A view toggle, so it stays out of m_toolGroup and works alongside any
    // tool. Off for every new window, and one state for all open images.
    m_gridAction = bar->addAction(QIcon(iconDir + "grid.svg"), tr("Grid"));
    m_gridAction->setIconText(tr("Grid"));
    m_gridAction->setProperty("phramerIcon", QStringLiteral("grid.svg"));
    m_gridAction->setCheckable(true);
    m_gridAction->setChecked(false);
    m_gridAction->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_Apostrophe));
    m_gridAction->setToolTip(withShortcut(
      tr("Show grid lines. View only: never saved or copied"),
      m_gridAction->shortcut().toString(QKeySequence::NativeText)));
    connect(m_gridAction, &QAction::toggled, this, [this](bool visible) {
        for (EditorCanvas* canvas : std::as_const(m_canvases)) {
            canvas->setGridVisible(visible);
        }
    });

    for (CaptureTool::Type type : editorToolTypes()) {
        CaptureTool* prototype = ToolFactory().CreateTool(type, this);
        if (!prototype) {
            continue;
        }
        QAction* action =
          bar->addAction(prototype->icon(background, true), prototype->name());
        // The shape button prefers a key of its own, and otherwise lists
        // the keys of the three tools it absorbed
        QString key =
          type == CaptureTool::TYPE_SHAPE ? shortcutFor(type) : QString();
        if (key.isEmpty()) {
            key = editorShortcutHint(type);
        }
        action->setIconText(ToolFactory::labelWithShortcut(type, key));
        action->setProperty("phramerTool", static_cast<int>(type));
        action->setCheckable(true);
        action->setData(static_cast<int>(type));
        action->setToolTip(
          withShortcut(prototype->description(), editorShortcutHint(type)));
        m_toolGroup->addAction(action);
        m_toolActions.insert(static_cast<int>(type), action);
        connect(action,
                &QAction::triggered,
                this,
                &EditorWindow::onToolActionTriggered);

        if (prototype->hasOptionsMenu()) {
            attachOptionsMenu(bar, action, type, background);
        }
        delete prototype;
    }

    // Drawing on the left, everything that acts on the image on the right
    auto* spacer = new QWidget(bar);
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    bar->addWidget(spacer);

    m_undoAction =
      bar->addAction(QIcon(iconDir + "undo-variant.svg"), tr("Undo"));
    m_undoAction->setProperty("phramerIcon",
                              QStringLiteral("undo-variant.svg"));
    m_undoAction->setShortcut(QKeySequence::Undo);
    connect(m_undoAction, &QAction::triggered, this, [this]() {
        if (EditorCanvas* canvas = currentCanvas()) {
            canvas->undoStack()->undo();
        }
    });

    m_redoAction =
      bar->addAction(QIcon(iconDir + "redo-variant.svg"), tr("Redo"));
    m_redoAction->setProperty("phramerIcon",
                              QStringLiteral("redo-variant.svg"));
    m_redoAction->setShortcut(QKeySequence::Redo);
    connect(m_redoAction, &QAction::triggered, this, [this]() {
        if (EditorCanvas* canvas = currentCanvas()) {
            canvas->undoStack()->redo();
        }
    });

    bar->addSeparator();

    m_colorAction = bar->addAction(tr("Color"));
    m_colorAction->setToolTip(tr("Drawing color"));
    connect(
      m_colorAction, &QAction::triggered, this, &EditorWindow::chooseColor);

    auto* sizeLabel = new QLabel(tr("Size"), bar);
    sizeLabel->setContentsMargins(6, 0, 4, 0);
    bar->addWidget(sizeLabel);
    m_sizeBox = new QSpinBox(bar);
    m_sizeBox->setRange(EditorCanvas::MinToolSize, EditorCanvas::MaxToolSize);
    m_sizeBox->setValue(ConfigHandler().drawThickness());
    m_sizeBox->setToolTip(tr("Thickness of the active tool"));
    connect(m_sizeBox, &QSpinBox::valueChanged, this, [this](int value) {
        if (EditorCanvas* canvas = currentCanvas()) {
            canvas->setToolSize(value);
        }
    });
    bar->addWidget(m_sizeBox);

    bar->addSeparator();

    QAction* copyAction =
      bar->addAction(QIcon(iconDir + "content-copy.svg"), tr("Copy"));
    copyAction->setProperty("phramerIcon", QStringLiteral("content-copy.svg"));
    copyAction->setToolTip(tr("Copy the current image to the clipboard"));
    copyAction->setShortcut(QKeySequence::Copy);
    connect(copyAction, &QAction::triggered, this, &EditorWindow::copyCurrent);

    // Save All is the button: a session usually holds several captures.
    // Saving just the image on screen is in its drop-down, and keeps Ctrl+S.
    const QKeySequence saveAllKey(Qt::CTRL | Qt::SHIFT | Qt::Key_S);
    QAction* saveAllAction =
      bar->addAction(QIcon(iconDir + "content-save.svg"), tr("Save All"));
    saveAllAction->setProperty("phramerIcon",
                               QStringLiteral("content-save.svg"));
    saveAllAction->setShortcut(saveAllKey);
    saveAllAction->setToolTip(
      withShortcut(tr("Save every image in this session to one folder"),
                   saveAllKey.toString(QKeySequence::NativeText)));
    connect(saveAllAction, &QAction::triggered, this, &EditorWindow::saveAll);

    auto* saveAction = new QAction(tr("Save This Image"), this);
    saveAction->setToolTip(tr("Save the current image"));
    saveAction->setShortcut(QKeySequence::Save);
    connect(saveAction, &QAction::triggered, this, &EditorWindow::saveCurrent);
    // A menu that is not open does not deliver shortcuts, so the window
    // carries the action as well
    addAction(saveAction);
    if (auto* saveButton =
          qobject_cast<QToolButton*>(bar->widgetForAction(saveAllAction))) {
        auto* saveMenu = new QMenu(saveButton);
        saveMenu->addAction(saveAction);
        saveButton->setMenu(saveMenu);
        saveButton->setPopupMode(QToolButton::MenuButtonPopup);
    }

    // Separated from Save so a destructive button is not adjacent to the one
    // people reach for most. No shortcut on purpose: Delete already removes
    // the selected annotation inside the canvas, and overloading it would
    // make a misfire cost the whole image.
    bar->addSeparator();
    QAction* removeAction =
      bar->addAction(QIcon(iconDir + "delete.svg"), tr("Remove Image"));
    removeAction->setIconText(tr("Remove"));
    removeAction->setProperty("phramerIcon", QStringLiteral("delete.svg"));
    removeAction->setToolTip(
      tr("Remove the current image from this editor session"));
    connect(removeAction,
            &QAction::triggered,
            this,
            &EditorWindow::removeCurrentImage);

#if defined(Q_OS_WIN)
    bar->addSeparator();
    QAction* ocrAction = bar->addAction(QIcon(iconDir + "ocr.svg"), tr("OCR"));
    ocrAction->setProperty("phramerIcon", QStringLiteral("ocr.svg"));
    ocrAction->setIconText(ToolFactory::labelWithShortcut(
      CaptureTool::TYPE_OCR, shortcutFor(CaptureTool::TYPE_OCR)));
    ocrAction->setToolTip(
      withShortcut(tr("Extract text from the current image"),
                   shortcutFor(CaptureTool::TYPE_OCR)));
    connect(ocrAction, &QAction::triggered, this, &EditorWindow::runOcr);
#endif
}

#if defined(Q_OS_WIN)
void EditorWindow::runOcr()
{
    EditorCanvas* canvas = currentCanvas();
    if (!canvas) {
        return;
    }

    // One at a time, by request: the results window is non-modal and carries
    // no indication of which image it read, so two of them would be
    // ambiguous. Raise the open one rather than silently doing nothing.
    if (!m_ocrWindow.isNull()) {
        m_ocrWindow->raise();
        m_ocrWindow->activateWindow();
        statusBar()->showMessage(
          tr("Close the open OCR window before running OCR on another image."),
          4000);
        return;
    }

    // The clean capture, not rendered(): an arrow, blur or text drawn over
    // the page would otherwise be handed to the recognizer as if it were
    // part of it. Same reason OcrTool reads origScreenshot.
    // The screen rect is empty because this image is no longer on screen —
    // it may be from a capture taken minutes ago.
    m_ocrWindow = new OcrResultsWindow(canvas->original(), QRect());
    m_ocrWindow->setAttribute(Qt::WA_DeleteOnClose);
    m_ocrWindow->show();
    m_ocrWindow->activateWindow();
}
#endif

void EditorWindow::attachOptionsMenu(QToolBar* bar,
                                     QAction* action,
                                     CaptureTool::Type type,
                                     const QColor& background)
{
    auto* button = qobject_cast<QToolButton*>(bar->widgetForAction(action));
    if (!button) {
        return;
    }
    // Split button rather than the overlay's click-opens-picker behaviour:
    // here there is room for an arrow, so re-selecting the tool does not
    // have to go through the menu
    button->setPopupMode(QToolButton::MenuButtonPopup);

    Q_UNUSED(background)
    // Reads the current background rather than capturing one, so a picker
    // opened after Windows switched theme still gets the right icon set
    const auto refreshIcon = [this, action, type]() {
        CaptureTool* tool = ToolFactory().CreateTool(type, nullptr);
        if (tool) {
            action->setIcon(tool->icon(m_toolbarBackground, true));
            delete tool;
        }
    };
    QMenu* menu = ShapeTool::buildMenu(button, refreshIcon);
    button->setMenu(menu);
    // Picking a variant is also a request to use the tool
    connect(menu, &QMenu::aboutToHide, this, [this, action]() {
        action->setChecked(true);
        onToolActionTriggered();
    });
}

void EditorWindow::buildStatusBar()
{
    statusBar()->setObjectName(QStringLiteral("editorStatus"));
    statusBar()->setSizeGripEnabled(false);
    auto* container = new QWidget(this);
    auto* layout = new QHBoxLayout(container);
    layout->setContentsMargins(6, 0, 6, 0);

    m_prevButton = new QPushButton(
      style()->standardIcon(QStyle::SP_ArrowLeft), tr("Previous"), container);
    m_nextButton = new QPushButton(
      style()->standardIcon(QStyle::SP_ArrowRight), tr("Next"), container);
    connect(
      m_prevButton, &QPushButton::clicked, this, &EditorWindow::showPrevious);
    connect(m_nextButton, &QPushButton::clicked, this, &EditorWindow::showNext);

    m_positionLabel = new QLabel(container);
    m_zoomLabel = new QLabel(container);

    layout->addWidget(m_prevButton);
    layout->addWidget(m_nextButton);
    layout->addSpacing(12);
    layout->addWidget(m_positionLabel);
    layout->addStretch();
    layout->addWidget(m_zoomLabel);

    statusBar()->addPermanentWidget(container, 1);
}

void EditorWindow::setCurrentIndex(int index)
{
    if (index < 0 || index >= m_canvases.size()) {
        return;
    }
    // Leaving an image must not abandon a half-typed text box
    if (EditorCanvas* previous = currentCanvas()) {
        if (previous != m_canvases.at(index)) {
            previous->commitActiveTool();
        }
    }

    m_currentIndex = index;
    m_pages->setCurrentIndex(index);
    m_filmstrip->setActiveIndex(index);

    EditorCanvas* canvas = m_canvases.at(index);
    canvas->setFocus();
    m_sizeBox->blockSignals(true);
    m_sizeBox->setValue(canvas->toolSize());
    m_sizeBox->blockSignals(false);

    // Zoom is per canvas, so the readout has to follow the shown image rather
    // than waiting for the next change
    onZoomChanged(canvas->zoom());

    // The toolbar is shared, so the newly shown canvas has to adopt whatever
    // tool is currently selected
    onToolActionTriggered();
    updateNavigationState();
    updateUndoState();
}

EditorCanvas* EditorWindow::currentCanvas() const
{
    if (m_currentIndex < 0 || m_currentIndex >= m_canvases.size()) {
        return nullptr;
    }
    return m_canvases.at(m_currentIndex);
}

void EditorWindow::onToolActionTriggered()
{
    EditorCanvas* canvas = currentCanvas();
    if (!canvas || !m_toolGroup->checkedAction()) {
        return;
    }
    const int data = m_toolGroup->checkedAction()->data().toInt();
    // Zoom draws nothing, so the canvas goes into the same no-tool state as
    // select mode; the mode itself only changes what the wheel does
    const auto type = data == ZoomActionData
                        ? CaptureTool::NONE
                        : static_cast<CaptureTool::Type>(data);
    canvas->setZoomMode(data == ZoomActionData);
    canvas->setActiveToolType(type);
    m_sizeBox->blockSignals(true);
    m_sizeBox->setValue(canvas->toolSize());
    m_sizeBox->blockSignals(false);
}

void EditorWindow::zoomCurrentCanvas(int notches, const QPoint& anchorInCanvas)
{
    EditorCanvas* canvas = currentCanvas();
    if (!canvas) {
        return;
    }
    auto* scroll = qobject_cast<QScrollArea*>(m_pages->currentWidget());
    if (!scroll) {
        canvas->zoomBy(notches, anchorInCanvas);
        return;
    }

    // Where the anchor sits in the viewport, measured before the zoom changes
    // the canvas geometry. Asking the widget rather than subtracting scroll
    // bar values matters because the scroll area is centre-aligned: a canvas
    // smaller than the viewport is offset by the centring margin, which no
    // scroll bar reports.
    const QPoint viewportAnchor =
      canvas->mapTo(scroll->viewport(), anchorInCanvas);

    if (!canvas->zoomBy(notches, anchorInCanvas)) {
        return;
    }

    // Put the image point that was under the pointer back under it. Without
    // this the view drifts away from whatever the user was looking at, which
    // is the whole reason to zoom at the cursor rather than at the centre.
    const QPoint want = canvas->fromImage(canvas->lastZoomAnchor());
    scroll->horizontalScrollBar()->setValue(want.x() - viewportAnchor.x());
    scroll->verticalScrollBar()->setValue(want.y() - viewportAnchor.y());
}

// Keyboard zoom has no pointer to anchor on, so it holds the middle of the
// visible area instead.
QPoint EditorWindow::viewportCentre() const
{
    auto* scroll = qobject_cast<QScrollArea*>(m_pages->currentWidget());
    EditorCanvas* canvas = currentCanvas();
    if (!scroll || !canvas) {
        return {};
    }
    // Mapped through the widget for the same reason as above, then pulled
    // inside the canvas: with a small image most of the viewport is empty
    // backdrop, and a point out there is not on the image at all.
    const QPoint centre =
      canvas->mapFrom(scroll->viewport(),
                      QPoint(scroll->viewport()->width() / 2,
                             scroll->viewport()->height() / 2));
    return { qBound(0, centre.x(), qMax(0, canvas->width() - 1)),
             qBound(0, centre.y(), qMax(0, canvas->height() - 1)) };
}

void EditorWindow::onZoomChanged(qreal zoom)
{
    if (m_zoomLabel) {
        m_zoomLabel->setText(tr("Zoom %1%").arg(qRound(zoom * 100)));
    }
}

void EditorWindow::keyPressEvent(QKeyEvent* event)
{
    switch (event->key()) {
        // A modifier on its own is not a shortcut, and matching one would
        // fire the moment the user reaches for Ctrl+S
        case Qt::Key_Control:
        case Qt::Key_Shift:
        case Qt::Key_Alt:
        case Qt::Key_Meta:
            break;
        default: {
            // Keypad origin is not part of a configured sequence, so a key
            // pressed on the number pad has to compare equal to the same key
            // on the main block
            const QKeyCombination combination = event->keyCombination();
            const QKeySequence pressed(QKeyCombination(
              combination.keyboardModifiers() & ~Qt::KeypadModifier,
              combination.key()));
            if (activateToolShortcut(pressed)) {
                event->accept();
                return;
            }
            break;
        }
    }
    QMainWindow::keyPressEvent(event);
}

bool EditorWindow::activateToolShortcut(const QKeySequence& pressed)
{
    if (pressed.isEmpty()) {
        return false;
    }

    const auto bound = [&pressed](CaptureTool::Type type) {
        const QString configured = shortcutFor(type);
        return !configured.isEmpty() && QKeySequence(configured) == pressed;
    };

    // Configured keys are matched first, so remapping a tool onto one of the
    // editor's own fallbacks below takes precedence over that fallback
    for (auto it = m_toolActions.constBegin(); it != m_toolActions.constEnd();
         ++it) {
        if (bound(static_cast<CaptureTool::Type>(it.key()))) {
            selectToolAction(it.value());
            return true;
        }
    }

    // The shape button replaced three tools that still own their shortcuts,
    // so each key has to land on the variant its tool used to draw. Both
    // halves of the variant are written, not just the kind: leaving the style
    // alone makes R draw whatever was last picked from the menu instead of
    // the filled rectangle it draws in the capture overlay.
    if (QAction* shape = m_toolActions.value(CaptureTool::TYPE_SHAPE)) {
        for (const ShapeBinding& binding : shapeBindings()) {
            if (!bound(binding.key)) {
                continue;
            }
            ConfigHandler config;
            config.setShapeKind(static_cast<int>(binding.kind));
            config.setShapeStyle(static_cast<int>(binding.style));
            refreshShapeIcon();
            selectToolAction(shape);
            return true;
        }
    }

#if defined(Q_OS_WIN)
    if (bound(CaptureTool::TYPE_OCR)) {
        runOcr();
        return true;
    }
#endif

    if (bound(CaptureTool::TYPE_MOVE_OBJECT) ||
        pressed == QKeySequence(QKeyCombination(LegacySelectToolKey))) {
        selectToolAction(m_selectAction);
        return true;
    }
    if (pressed == QKeySequence(QKeyCombination(CircleCountKey))) {
        selectToolAction(m_toolActions.value(CaptureTool::TYPE_CIRCLECOUNT));
        return true;
    }
    return false;
}

void EditorWindow::selectToolAction(QAction* action)
{
    if (!action) {
        return;
    }
    action->setChecked(true);
    onToolActionTriggered();
    // Drawing happens on the canvas, so the keystroke should leave the focus
    // there rather than wherever it happened to be
    if (EditorCanvas* canvas = currentCanvas()) {
        canvas->setFocus();
    }
}

void EditorWindow::refreshShapeIcon()
{
    QAction* action = m_toolActions.value(CaptureTool::TYPE_SHAPE);
    if (!action) {
        return;
    }
    CaptureTool* tool =
      ToolFactory().CreateTool(CaptureTool::TYPE_SHAPE, nullptr);
    if (tool) {
        action->setIcon(tool->icon(m_toolbarBackground, true));
        delete tool;
    }
}

void EditorWindow::removeCurrentImage()
{
    if (m_currentIndex < 0 || m_currentIndex >= m_canvases.size()) {
        return;
    }
    EditorCanvas* canvas = m_canvases.at(m_currentIndex);

    // A clean image goes without ceremony; one carrying annotations that were
    // never saved asks first, using the same clean-state test as the window's
    // own close prompt.
    if (canvas->isDirty()) {
        const auto answer = QMessageBox::question(
          this,
          tr("Remove Image"),
          tr("Image %1 of %2 has annotations that have not been saved. "
             "Remove it anyway?")
            .arg(m_currentIndex + 1)
            .arg(m_canvases.size()),
          QMessageBox::Discard | QMessageBox::Cancel,
          QMessageBox::Cancel);
        if (answer != QMessageBox::Discard) {
            return;
        }
    }

    const int removed = m_currentIndex;

    // Last one out closes the session: an editor with no image has nothing to
    // show and no way to get one back.
    if (m_canvases.size() == 1) {
        m_canvases.clear();
        close();
        return;
    }

    // One removal from each of the three parallel lists keeps the order of
    // everything else intact
    m_canvases.removeAt(removed);
    QWidget* page = m_pages->widget(removed);
    m_pages->removeWidget(page);
    page->deleteLater();
    m_filmstrip->removeImage(removed);

    // Show the image that took its place, or the new last one if the removed
    // image was at the end
    m_currentIndex = -1;
    setCurrentIndex(qMin(removed, m_canvases.size() - 1));
    updateNavigationState();
}

void EditorWindow::onImagesReordered(int from, int to)
{
    if (from == to || from < 0 || from >= m_canvases.size() || to < 0 ||
        to >= m_canvases.size()) {
        return;
    }

    m_canvases.move(from, to);
    // QStackedWidget has no move of its own. Removing a page detaches it
    // without deleting it, which is what makes reinserting it safe.
    QWidget* page = m_pages->widget(from);
    m_pages->removeWidget(page);
    m_pages->insertWidget(to, page);

    // The dragged image is the one on screen: pressing a tile to start the
    // drag already navigated to it, and the strip leaves it selected
    m_currentIndex = to;
    m_pages->setCurrentIndex(to);
    updateNavigationState();

    // A drag leaves the focus on the strip, and QListWidget answers a plain
    // letter with its own keyboard search — so without this the tool keys go
    // dead until the canvas is clicked again
    if (EditorCanvas* canvas = currentCanvas()) {
        canvas->setFocus();
    }
}

void EditorWindow::onCanvasContentChanged()
{
    if (sender() != currentCanvas()) {
        return;
    }
    m_thumbnailTimer->start();
    updateUndoState();
}

void EditorWindow::updateNavigationState()
{
    const int total = m_canvases.size();
    m_prevButton->setEnabled(m_currentIndex > 0);
    m_nextButton->setEnabled(m_currentIndex >= 0 && m_currentIndex < total - 1);
    m_positionLabel->setText(
      total == 0 ? QString()
                 : tr("Image %1 of %2").arg(m_currentIndex + 1).arg(total));
}

void EditorWindow::updateUndoState()
{
    EditorCanvas* canvas = currentCanvas();
    m_undoAction->setEnabled(canvas && canvas->undoStack()->canUndo());
    m_redoAction->setEnabled(canvas && canvas->undoStack()->canRedo());
}

void EditorWindow::applyTheme()
{
    const EditorTheme theme = EditorTheme::current();
    setPalette(theme.palette(QGuiApplication::palette()));
    setStyleSheet(theme.styleSheet());
    m_toolbarBackground = theme.window;
    refreshToolbarIcons();
    updateColorSwatch();
}

void EditorWindow::refreshToolbarIcons()
{
    if (!m_toolBar) {
        return;
    }
    const QString iconDir = ColorUtils::colorIsDark(m_toolbarBackground)
                              ? PathInfo::whiteIconPath()
                              : PathInfo::blackIconPath();
    for (QAction* action : m_toolBar->actions()) {
        const QVariant file = action->property("phramerIcon");
        if (file.isValid()) {
            action->setIcon(QIcon(iconDir + file.toString()));
            continue;
        }
        const QVariant type = action->property("phramerTool");
        if (!type.isValid()) {
            continue;
        }
        CaptureTool* tool = ToolFactory().CreateTool(
          static_cast<CaptureTool::Type>(type.toInt()), nullptr);
        if (tool) {
            action->setIcon(tool->icon(m_toolbarBackground, true));
            delete tool;
        }
    }
}

void EditorWindow::showEvent(QShowEvent* event)
{
    QMainWindow::showEvent(event);
    if (m_fadedIn) {
        return;
    }
    m_fadedIn = true;
    // A short fade rather than the window popping in fully drawn. Short
    // enough that it never delays the first click.
    setWindowOpacity(0.0);
    auto* fade = new QPropertyAnimation(this, "windowOpacity", this);
    fade->setDuration(160);
    fade->setStartValue(0.0);
    fade->setEndValue(1.0);
    fade->setEasingCurve(QEasingCurve::OutCubic);
    fade->start(QAbstractAnimation::DeleteWhenStopped);
}

void EditorWindow::updateColorSwatch()
{
    const QColor color = ConfigHandler().drawColor();
    const qreal ratio = devicePixelRatioF();
    QPixmap swatch(QSize(20, 20) * ratio);
    swatch.setDevicePixelRatio(ratio);
    swatch.fill(Qt::transparent);
    QPainter painter(&swatch);
    painter.setRenderHint(QPainter::Antialiasing);
    // A hairline rim so a swatch that matches the toolbar still shows
    const QColor rim = ColorUtils::colorIsDark(m_toolbarBackground)
                         ? QColor(255, 255, 255, 90)
                         : QColor(0, 0, 0, 70);
    painter.setPen(QPen(rim, 1));
    painter.setBrush(color);
    painter.drawEllipse(QRectF(2.5, 2.5, 15, 15));
    painter.end();
    m_colorAction->setIcon(QIcon(swatch));
}

void EditorWindow::chooseColor()
{
    const QColor chosen = QColorDialog::getColor(
      ConfigHandler().drawColor(), this, tr("Drawing color"));
    if (!chosen.isValid()) {
        return;
    }
    if (EditorCanvas* canvas = currentCanvas()) {
        canvas->setDrawColor(chosen);
    } else {
        ConfigHandler().setDrawColor(chosen);
    }
    updateColorSwatch();
}

void EditorWindow::showPrevious()
{
    setCurrentIndex(m_currentIndex - 1);
}

void EditorWindow::showNext()
{
    setCurrentIndex(m_currentIndex + 1);
}

void EditorWindow::copyCurrent()
{
    EditorCanvas* canvas = currentCanvas();
    if (!canvas) {
        return;
    }
    canvas->commitActiveTool();
    FlameshotDaemon::copyToClipboard(canvas->rendered());
}

void EditorWindow::saveCurrent()
{
    EditorCanvas* canvas = currentCanvas();
    if (!canvas) {
        return;
    }
    canvas->commitActiveTool();
    if (saveToFilesystemGUI(canvas->rendered())) {
        canvas->markSaved();
    }
}

void EditorWindow::saveAll()
{
    if (m_canvases.isEmpty()) {
        return;
    }

    QString start = ConfigHandler().savePath();
    if (start.isEmpty() || !QDir(start).exists()) {
        start =
          QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
    }
    const QString parent = QFileDialog::getExistingDirectory(
      this,
      tr("Save all images to folder"),
      start,
      QFileDialog::ShowDirsOnly | QFileDialog::DontResolveSymlinks);
    if (parent.isEmpty()) {
        return;
    }

    // A session is a set, so it gets a folder of its own rather than being
    // tipped loose into whatever the user picked
    const QString folder = createSessionFolder(parent);
    if (folder.isEmpty()) {
        QMessageBox::warning(this,
                             tr("Save All"),
                             tr("Could not create a folder inside %1.")
                               .arg(QDir::toNativeSeparators(parent)));
        return;
    }

    // One timestamp pattern for the whole batch, so the numeric prefix is
    // what actually distinguishes the files and session order survives an
    // alphabetical listing
    const QString pattern = FileNameHandler().parsedPattern();
    int failures = 0;
    for (int i = 0; i < m_canvases.size(); ++i) {
        EditorCanvas* canvas = m_canvases.at(i);
        canvas->commitActiveTool();
        const QString target =
          QStringLiteral("%1/%2 - %3").arg(folder).arg(i + 1).arg(pattern);
        if (saveToFilesystem(canvas->rendered(), target)) {
            canvas->markSaved();
        } else {
            ++failures;
        }
    }

    if (failures > 0) {
        QMessageBox::warning(this,
                             tr("Save All"),
                             tr("%1 of %2 images could not be saved to %3.")
                               .arg(failures)
                               .arg(m_canvases.size())
                               .arg(QDir::toNativeSeparators(folder)));
    } else {
        AbstractLogger::info().attachNotificationPath(folder)
          << tr("%1 images saved to %2")
               .arg(m_canvases.size())
               .arg(QDir::toNativeSeparators(folder));
    }
}

QString EditorWindow::createSessionFolder(const QString& parent)
{
    QDir base(parent);
    // Sortable and unambiguous: two sessions saved a minute apart never
    // collide, and the folders list in the order they were captured
    const QString stamp = QDateTime::currentDateTime().toString(
      QStringLiteral("yyyy-MM-dd HH-mm-ss"));
    QString name = tr("Phramer %1").arg(stamp);

    // Same-second collisions are possible, and mkdir on an existing folder
    // would silently reuse it and mix two sessions together
    QString candidate = name;
    for (int suffix = 1; base.exists(candidate); ++suffix) {
        candidate = QStringLiteral("%1_%2").arg(name).arg(suffix);
    }
    if (!base.mkpath(candidate)) {
        return {};
    }
    return base.absoluteFilePath(candidate);
}

bool EditorWindow::hasUnsavedWork() const
{
    for (EditorCanvas* canvas : m_canvases) {
        if (canvas->isDirty()) {
            return true;
        }
    }
    return false;
}

void EditorWindow::closeEvent(QCloseEvent* event)
{
    if (hasUnsavedWork()) {
        const auto answer = QMessageBox::question(
          this,
          tr("Close Editor"),
          tr("This session has annotations that have not been saved. "
             "Closing the editor discards every image in it. Close anyway?"),
          QMessageBox::Yes | QMessageBox::No,
          QMessageBox::No);
        if (answer != QMessageBox::Yes) {
            event->ignore();
            return;
        }
    }
    QMainWindow::closeEvent(event);
}
