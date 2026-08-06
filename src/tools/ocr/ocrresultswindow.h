// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include "tools/ocr/ocrengine.h"

#include <QPixmap>
#include <QRect>
#include <QWidget>

class QComboBox;
class QLabel;
class QPlainTextEdit;
class QPushButton;
class QTimer;
class LoadSpinner;

/**
 * @brief Recognizes one image in a worker thread.
 *
 * Communicates purely via a queued signal, so the results window can be
 * closed while recognition is still running.
 */
class OcrWorker : public QObject
{
    Q_OBJECT
public:
    OcrWorker(QImage image, QString language, quint64 runId);

public slots:
    void process();

signals:
    void finished(const OcrResult& result, quint64 runId);

private:
    QImage m_image;
    QString m_language;
    quint64 m_runId;
};

/**
 * @brief Lists the installed OCR languages in a worker thread.
 *
 * Creating an engine initializes a COM apartment and queries WinRT, which is
 * slow enough to stall the window while it is being built. The language row
 * therefore appears once this reports back rather than being there from the
 * start.
 */
class OcrLanguageProbe : public QObject
{
    Q_OBJECT

public slots:
    void process();

signals:
    void finished(const QStringList& languages);
};

/**
 * @brief Non-modal window showing the text extracted from a capture.
 */
class OcrResultsWindow : public QWidget
{
    Q_OBJECT
public:
    explicit OcrResultsWindow(const QPixmap& capture,
                              const QRect& screenRect,
                              QWidget* parent = nullptr);

private slots:
    void copyAll();
    void onLanguageChanged(int index);
    void onLayoutChanged(int index);
    void onLanguagesProbed(const QStringList& languages);
    void onWorkerFinished(const OcrResult& result, quint64 runId);

private:
    void startRecognition();
    // Rebuild the text view from the stored result. Changing how lines are
    // joined must not cost another recognition pass.
    void refreshText();
    void showStatus(const OcrResult& result);

    QPixmap m_capture;
    QRect m_screenRect;
    OcrResult m_result;
    bool m_hasResult{ false };
    // Effective BCP-47 tag; empty means the system default. Kept here rather
    // than read from the combo, which does not exist until the probe returns.
    QString m_language;

    QPlainTextEdit* m_textEdit{ nullptr };
    QWidget* m_languageRow{ nullptr };
    QComboBox* m_languageBox{ nullptr };
    QComboBox* m_layoutBox{ nullptr };
    QLabel* m_hintLabel{ nullptr };
    QLabel* m_statusLabel{ nullptr };
    QPushButton* m_copyButton{ nullptr };
    LoadSpinner* m_spinner{ nullptr };
    QTimer* m_busyDelay{ nullptr };

    // Increased on every recognition start; stale worker results are
    // discarded by comparing against it
    quint64 m_runId{ 0 };
};
