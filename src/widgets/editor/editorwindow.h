// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/capturetool.h"

#include <QColor>
#include <QHash>
#include <QKeySequence>
#include <QMainWindow>
#include <QPointer>
#include <QVector>

class EditorCanvas;
class EditorFilmstrip;
#if defined(Q_OS_WIN)
class OcrResultsWindow;
#endif
class QAction;
class QActionGroup;
class QLabel;
class QPushButton;
class QScrollArea;
class QSpinBox;
class QStackedWidget;
class QTimer;
class QToolBar;
class QToolButton;

/**
 * @brief The one standalone editor window, holding a session of captures.
 *
 * Exactly one instance exists at a time. A capture routed here while the
 * window is open is appended to the running session; if the window is closed
 * the capture opens a fresh one with that image first. Each image gets its
 * own EditorCanvas, so undo history and placed objects never cross between
 * images.
 */
class EditorWindow : public QMainWindow
{
    Q_OBJECT
public:
    ~EditorWindow() override;

    // Appends to the open session, or opens the window if it is closed. The
    // only entry point the rest of the application should use. objects are
    // annotations still editable from the capture overlay, and offset maps
    // their coordinates onto capture; see EditorCanvas::adoptObjects.
    static void addCapture(const QPixmap& capture,
                           const QList<QPointer<CaptureTool>>& objects = {},
                           const QPoint& offset = QPoint());
    static bool isOpen();
    // Opens the window with no image, showing how to capture one. For the
    // welcome's "Take me to Phramer".
    static void openEmpty();

    void addImage(const QPixmap& image,
                  const QList<QPointer<CaptureTool>>& objects = {},
                  const QPoint& offset = QPoint());
    int imageCount() const { return m_canvases.size(); }

protected:
    void closeEvent(QCloseEvent* event) override;
    // Tool keys are matched here rather than through QAction::setShortcut. A
    // window shortcut fires wherever the focus is, so typing "p" into a text
    // annotation would swap to the pencil mid-word; an unhandled key event
    // only reaches this window because the focused widget did not want it.
    void keyPressEvent(QKeyEvent* event) override;
    // Fades the window in the first time it appears
    void showEvent(QShowEvent* event) override;

private slots:
    void copyCurrent();
    void copyCurrentAsFile();
    void saveCurrent();
    void saveAll();
    void showPrevious();
    void showNext();
    void chooseColor();
    void onToolActionTriggered();
    // Applies a wheel-notch zoom to the current canvas and keeps the image
    // point under the pointer where it was
    void zoomCurrentCanvas(int notches, const QPoint& anchorInCanvas);
    void onZoomChanged(qreal zoom);
    QPoint viewportCentre() const;
    // Removes the image on show, closing the window when it was the last one
    void removeCurrentImage();
    void onCanvasContentChanged();
    void onImagesReordered(int from, int to);
#if defined(Q_OS_WIN)
    void runOcr();
#endif

private:
    explicit EditorWindow(QWidget* parent = nullptr);

    // Opens the window at the capture's own size where the screen allows it,
    // so a capture that would fit does not start out scrolled
    void resizeToFit(EditorCanvas* canvas);
    void buildToolBar();
    // Applies the light or dark look to match Windows, and re-picks every
    // toolbar icon for it. Runs again whenever Windows switches.
    void applyTheme();
    void refreshToolbarIcons();
    void buildEmptyState();
    // Swaps between the empty state and the canvas, and rewrites the hint
    // from the current shortcut settings
    void updateEmptyState();
    // Turns a toolbar entry into a split button whose arrow opens the tool's
    // variant picker
    void attachOptionsMenu(QToolButton* button,
                           QAction* action,
                           CaptureTool::Type type,
                           const QColor& background);
    void buildStatusBar();
    // Selects the tool the key is bound to, if any. Configured shortcuts win
    // over the editor's own fallbacks, so remapping a tool onto V or N in
    // Settings does what the user asked for.
    bool activateToolShortcut(const QKeySequence& pressed);
    void selectToolAction(QAction* action);
    // The shape button wears the icon of whichever variant is configured, so
    // the R and C keys have to repaint it after switching kinds
    void refreshShapeIcon();
    // Makes a uniquely named folder under parent to hold one Save All batch.
    // Empty on failure.
    QString createSessionFolder(const QString& parent);
    void setCurrentIndex(int index);
    EditorCanvas* currentCanvas() const;
    void updateNavigationState();
    void updateUndoState();
    void updateColorSwatch();
    bool hasUnsavedWork() const;

    static QPointer<EditorWindow> s_instance;

    QStackedWidget* m_pages{ nullptr };
    EditorFilmstrip* m_filmstrip{ nullptr };
    QVector<EditorCanvas*> m_canvases;
    int m_currentIndex{ -1 };

    QActionGroup* m_toolGroup{ nullptr };
    // Tool type to its toolbar entry, for keyboard selection. Keyed by int
    // because CaptureTool::Type is not a hashable key without a qHash.
    QHash<int, QAction*> m_toolActions;
    QAction* m_selectAction{ nullptr };
    // Zoom is a view mode, not a tool: it joins m_toolGroup so it is mutually
    // exclusive with drawing, but places no object
    QAction* m_zoomAction{ nullptr };
    QAction* m_gridAction{ nullptr };
    // Along the top: undo, redo, colour and size, then copy, save, remove
    // and OCR
    QToolBar* m_toolBar{ nullptr };
    // Under the picture: the annotation tools, wrapping onto more rows
    QWidget* m_annotationBar{ nullptr };
    // Shown in place of the canvas while the session holds no image
    QWidget* m_emptyState{ nullptr };
    QLabel* m_emptyHint{ nullptr };
    bool m_fadedIn{ false };
    // Window background the tool icons were chosen against, kept so an icon
    // can be rebuilt later without re-deriving it
    QColor m_toolbarBackground;
    QAction* m_undoAction{ nullptr };
    QAction* m_redoAction{ nullptr };
    QAction* m_colorAction{ nullptr };
    QSpinBox* m_sizeBox{ nullptr };
    QPushButton* m_prevButton{ nullptr };
    QPushButton* m_nextButton{ nullptr };
    QLabel* m_positionLabel{ nullptr };
    QLabel* m_zoomLabel{ nullptr };

    // Coalesces thumbnail regeneration; redrawing the strip on every stroke
    // of a 4K capture is what makes a filmstrip feel slow
    QTimer* m_thumbnailTimer{ nullptr };

#if defined(Q_OS_WIN)
    // One results window at a time. Recognition owns a worker thread and its
    // own COM apartment, and a second window would leave the user guessing
    // which image the text on screen came from.
    QPointer<OcrResultsWindow> m_ocrWindow;
#endif
};
