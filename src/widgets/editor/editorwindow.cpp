// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/editor/editorwindow.h"

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
#include <QHBoxLayout>
#include <QLabel>
#include <QMenu>
#include <QMessageBox>
#include <QPixmap>
#include <QPushButton>
#include <QScreen>
#include <QScrollArea>
#include <QSpinBox>
#include <QStackedWidget>
#include <QStandardPaths>
#include <QStatusBar>
#include <QStyle>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QVBoxLayout>

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

} // namespace

EditorWindow::EditorWindow(QWidget* parent)
  : QMainWindow(parent)
{
    setAttribute(Qt::WA_DeleteOnClose);
    setWindowTitle(tr("Phramer Editor"));
    setWindowIcon(GlobalValues::appIcon());
    resize(1000, 720);

    m_pages = new QStackedWidget(this);

    m_filmstrip = new EditorFilmstrip(this);
    connect(m_filmstrip,
            &EditorFilmstrip::imageActivated,
            this,
            [this](int index) { setCurrentIndex(index); });

    auto* central = new QWidget(this);
    auto* layout = new QVBoxLayout(central);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);
    layout->addWidget(m_pages, 1);
    layout->addWidget(m_filmstrip);
    setCentralWidget(central);

    buildToolBar();
    buildStatusBar();

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
}

EditorWindow::~EditorWindow()
{
    if (s_instance == this) {
        s_instance = nullptr;
    }
}

bool EditorWindow::isOpen()
{
    return !s_instance.isNull();
}

void EditorWindow::addCapture(const QPixmap& capture)
{
    if (s_instance.isNull()) {
        s_instance = new EditorWindow();
    }
    s_instance->addImage(capture);
    s_instance->show();
    s_instance->raise();
    s_instance->activateWindow();
}

void EditorWindow::addImage(const QPixmap& image)
{
    if (image.isNull()) {
        return;
    }

    auto* canvas = new EditorCanvas(image, this);
    connect(canvas,
            &EditorCanvas::contentChanged,
            this,
            &EditorWindow::onCanvasContentChanged);
    // The canvas has its own right-click colour picker, so the toolbar
    // swatch cannot assume it is the only thing that sets the colour
    connect(canvas, &EditorCanvas::drawColorChanged, this, [this]() {
        updateColorSwatch();
    });
    // The canvas is fixed to the image size; the scroll area handles captures
    // larger than the window
    auto* scroll = new QScrollArea(m_pages);
    scroll->setAlignment(Qt::AlignCenter);
    scroll->setWidget(canvas);
    scroll->setWidgetResizable(false);
    scroll->setBackgroundRole(QPalette::Dark);

    m_canvases.append(canvas);
    m_pages->addWidget(scroll);
    m_filmstrip->appendImage(image);

    // Only for the first image, and only before the window is up: after that
    // the size is the user's business
    if (m_canvases.size() == 1 && !isVisible()) {
        resizeToFit(canvas);
    }

    setCurrentIndex(m_canvases.size() - 1);
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
    bar->setMovable(false);
    bar->setToolButtonStyle(Qt::ToolButtonIconOnly);

    // Tool icons come in a light and a dark variant; pick the one that
    // contrasts with whatever palette the window is actually using
    const QColor background = palette().color(QPalette::Window);
    const QString iconDir = ColorUtils::colorIsDark(background)
                              ? PathInfo::whiteIconPath()
                              : PathInfo::blackIconPath();

    m_toolGroup = new QActionGroup(this);
    m_toolGroup->setExclusive(true);

    auto* selectAction =
      bar->addAction(QIcon(iconDir + "cursor-move.svg"), tr("Select and move"));
    selectAction->setCheckable(true);
    selectAction->setChecked(true);
    selectAction->setData(static_cast<int>(CaptureTool::NONE));
    selectAction->setToolTip(
      tr("Select, move and delete objects you have placed"));
    m_toolGroup->addAction(selectAction);
    connect(selectAction,
            &QAction::triggered,
            this,
            &EditorWindow::onToolActionTriggered);

    for (CaptureTool::Type type : editorToolTypes()) {
        CaptureTool* prototype = ToolFactory().CreateTool(type, this);
        if (!prototype) {
            continue;
        }
        QAction* action =
          bar->addAction(prototype->icon(background, true), prototype->name());
        action->setCheckable(true);
        action->setData(static_cast<int>(type));
        action->setToolTip(prototype->description());
        m_toolGroup->addAction(action);
        connect(action,
                &QAction::triggered,
                this,
                &EditorWindow::onToolActionTriggered);

        if (prototype->hasOptionsMenu()) {
            attachOptionsMenu(bar, action, type, background);
        }
        delete prototype;
    }

    bar->addSeparator();

    m_undoAction =
      bar->addAction(QIcon(iconDir + "undo-variant.svg"), tr("Undo"));
    m_undoAction->setShortcut(QKeySequence::Undo);
    connect(m_undoAction, &QAction::triggered, this, [this]() {
        if (EditorCanvas* canvas = currentCanvas()) {
            canvas->undoStack()->undo();
        }
    });

    m_redoAction =
      bar->addAction(QIcon(iconDir + "redo-variant.svg"), tr("Redo"));
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
    sizeLabel->setContentsMargins(8, 0, 4, 0);
    bar->addWidget(sizeLabel);
    m_sizeBox = new QSpinBox(bar);
    m_sizeBox->setRange(1, 100);
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
    copyAction->setToolTip(tr("Copy the current image to the clipboard"));
    copyAction->setShortcut(QKeySequence::Copy);
    connect(copyAction, &QAction::triggered, this, &EditorWindow::copyCurrent);

    QAction* saveAction =
      bar->addAction(QIcon(iconDir + "content-save.svg"), tr("Save"));
    saveAction->setToolTip(tr("Save the current image"));
    saveAction->setShortcut(QKeySequence::Save);
    connect(saveAction, &QAction::triggered, this, &EditorWindow::saveCurrent);

    QAction* saveAllAction = bar->addAction(tr("Save All"));
    saveAllAction->setToolTip(
      tr("Save every image in this session to one folder"));
    connect(saveAllAction, &QAction::triggered, this, &EditorWindow::saveAll);

#if defined(Q_OS_WIN)
    bar->addSeparator();
    QAction* ocrAction = bar->addAction(QIcon(iconDir + "ocr.svg"), tr("OCR"));
    ocrAction->setToolTip(tr("Extract text from the current image"));
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

    const auto refreshIcon = [action, type, background]() {
        CaptureTool* tool = ToolFactory().CreateTool(type, nullptr);
        if (tool) {
            action->setIcon(tool->icon(background, true));
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

    layout->addWidget(m_prevButton);
    layout->addWidget(m_nextButton);
    layout->addSpacing(12);
    layout->addWidget(m_positionLabel);
    layout->addStretch();

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
    const auto type = static_cast<CaptureTool::Type>(
      m_toolGroup->checkedAction()->data().toInt());
    canvas->setActiveToolType(type);
    m_sizeBox->blockSignals(true);
    m_sizeBox->setValue(canvas->toolSize());
    m_sizeBox->blockSignals(false);
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

void EditorWindow::updateColorSwatch()
{
    const QColor color = ConfigHandler().drawColor();
    QPixmap swatch(20, 20);
    swatch.fill(color);
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
