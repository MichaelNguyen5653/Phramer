// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/capturetool.h"

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
    // only entry point the rest of the application should use.
    static void addCapture(const QPixmap& capture);
    static bool isOpen();

    void addImage(const QPixmap& image);
    int imageCount() const { return m_canvases.size(); }

protected:
    void closeEvent(QCloseEvent* event) override;

private slots:
    void copyCurrent();
    void saveCurrent();
    void saveAll();
    void showPrevious();
    void showNext();
    void chooseColor();
    void onToolActionTriggered();
    void onCanvasContentChanged();
#if defined(Q_OS_WIN)
    void runOcr();
#endif

private:
    explicit EditorWindow(QWidget* parent = nullptr);

    // Opens the window at the capture's own size where the screen allows it,
    // so a capture that would fit does not start out scrolled
    void resizeToFit(EditorCanvas* canvas);
    void buildToolBar();
    // Turns a toolbar entry into a split button whose arrow opens the tool's
    // variant picker
    void attachOptionsMenu(QToolBar* bar,
                           QAction* action,
                           CaptureTool::Type type,
                           const QColor& background);
    void buildStatusBar();
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
    QAction* m_undoAction{ nullptr };
    QAction* m_redoAction{ nullptr };
    QAction* m_colorAction{ nullptr };
    QSpinBox* m_sizeBox{ nullptr };
    QPushButton* m_prevButton{ nullptr };
    QPushButton* m_nextButton{ nullptr };
    QLabel* m_positionLabel{ nullptr };

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
