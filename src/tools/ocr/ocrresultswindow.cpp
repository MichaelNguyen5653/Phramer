// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrresultswindow.h"

#include "tools/ocr/ocrlayout.h"
#include "tools/ocr/ocrpipeline.h"
#include "tools/ocr/ocrpreprocess.h"
#include "tools/ocr/uiatextreader.h"
#include "utils/confighandler.h"
#include "widgets/loadspinner.h"

#include <QApplication>
#include <QClipboard>
#include <QComboBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QLocale>
#include <QPalette>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScopedPointer>
#include <QSignalBlocker>
#include <QThread>
#include <QTimer>
#include <QVBoxLayout>

#include <utility>

namespace {

/// Run `worker` on a thread of its own and have both clean themselves up
/// once its finished() signal has been delivered.
template<typename Worker, typename Signal>
void startWorker(Worker* worker, Signal finishedSignal)
{
    auto* thread = new QThread();
    worker->moveToThread(thread);
    QObject::connect(thread, &QThread::started, worker, &Worker::process);
    QObject::connect(worker, finishedSignal, thread, &QThread::quit);
    QObject::connect(thread, &QThread::finished, worker, &QObject::deleteLater);
    QObject::connect(thread, &QThread::finished, thread, &QObject::deleteLater);
    thread->start();
}

} // namespace

OcrWorker::OcrWorker(QImage image,
                     QRect screenRect,
                     QString language,
                     quint64 runId,
                     OcrEffort effort)
  : m_image(std::move(image))
  , m_screenRect(screenRect)
  , m_language(std::move(language))
  , m_runId(runId)
  , m_effort(effort)
{}

void OcrWorker::process()
{
#ifdef Q_OS_WIN
    // A screenshot is a lossy encoding of text the source application still
    // holds verbatim. Where it can be read back, recognition has nothing to
    // add. High effort means the user was unhappy with a recognized result,
    // and this path has already declined once, so it is not retried.
    if (m_effort == OcrEffort::Normal) {
        OcrResult exact = readWindowText(m_screenRect);
        if (exact.status == OcrResult::Status::Ok) {
            exact.source = OcrResult::Source::Exact;
            exact.diagnostics =
              QStringLiteral("read directly from the window under the "
                             "selection; no recognition was needed");
            exact.lines = ocrOrderLines(exact.lines);
            emit finished(exact, m_runId);
            return;
        }
    }
#endif

    // The engine is created inside the worker thread so nothing is shared
    // with the GUI thread
    QScopedPointer<OcrEngine> engine(OcrEngine::create());
    OcrEngine* raw = engine.data();

    OcrResult result = ocrRunPipeline(
      m_image,
      [raw](const QImage& image, const QString& language) {
          return raw->recognize(image, language);
      },
      m_language,
      engine->maxImageDimension(),
      m_effort);

    // Ordering is deterministic, so doing it here keeps it off the GUI
    // thread and lets the window re-join the lines for free
    result.lines = ocrOrderLines(result.lines);

    emit finished(result, m_runId);
}

void OcrLanguageProbe::process()
{
    QScopedPointer<OcrEngine> engine(OcrEngine::create());
    emit finished(engine->availableLanguages());
}

OcrResultsWindow::OcrResultsWindow(const QPixmap& capture,
                                   const QRect& screenRect,
                                   QWidget* parent)
  : QWidget(parent, Qt::Window | Qt::WindowStaysOnTopHint)
  , m_capture(capture)
  , m_screenRect(screenRect)
{
    // The REQ_ADD_EXTERNAL_WIDGETS handler sets this too; set it here as
    // well so the window cannot leak if created some other way
    setAttribute(Qt::WA_DeleteOnClose);
    setWindowTitle(tr("Extracted text"));
    resize(520, 420);

    ConfigHandler config;
    m_language = config.ocrLanguage();

    auto* layout = new QVBoxLayout(this);

    // Recognition quality is decided by the capture, not by anything this
    // window can offer, so the advice belongs here — but it is a standing
    // hint rather than a message about this result, so it is tinted down
    // instead of styled like the status line below it.
    m_hintLabel = new QLabel(
      tr("To improve OCR result, capture image close up and make sure image "
         "is not blurry for best result"),
      this);
    m_hintLabel->setWordWrap(true);
    const QColor accent = palette().color(QPalette::Highlight);
    QColor hintText = palette().color(QPalette::WindowText);
    hintText.setAlpha(190);
    m_hintLabel->setStyleSheet(
      QStringLiteral("QLabel { background-color: rgba(%1,%2,%3,26);"
                     " border-left: 3px solid rgba(%1,%2,%3,150);"
                     " color: rgba(%4,%5,%6,%7);"
                     " padding: 6px 8px; }")
        .arg(accent.red())
        .arg(accent.green())
        .arg(accent.blue())
        .arg(hintText.red())
        .arg(hintText.green())
        .arg(hintText.blue())
        .arg(hintText.alpha()));
    layout->addWidget(m_hintLabel);

    auto* optionsLayout = new QHBoxLayout();

    // Built hidden: the installed languages are not known until the probe
    // running on a worker thread reports back
    m_languageRow = new QWidget(this);
    auto* langLayout = new QHBoxLayout(m_languageRow);
    langLayout->setContentsMargins(0, 0, 0, 0);
    langLayout->addWidget(new QLabel(tr("Language:"), m_languageRow));
    m_languageBox = new QComboBox(m_languageRow);
    connect(
      m_languageBox,
      static_cast<void (QComboBox::*)(int)>(&QComboBox::currentIndexChanged),
      this,
      &OcrResultsWindow::onLanguageChanged);
    langLayout->addWidget(m_languageBox);
    m_languageRow->hide();
    optionsLayout->addWidget(m_languageRow);

    optionsLayout->addWidget(new QLabel(tr("Layout:"), this));
    m_layoutBox = new QComboBox(this);
    m_layoutBox->addItem(tr("Preserve"), int(OcrTextLayout::Preserve));
    m_layoutBox->addItem(tr("Plain lines"), int(OcrTextLayout::Plain));
    m_layoutBox->addItem(tr("Join wrapped"), int(OcrTextLayout::Join));
    m_layoutBox->setToolTip(
      tr("Preserve keeps indentation and the gaps between columns, so code "
         "and terminal output paste back as they looked. Join merges lines "
         "that continue the same paragraph, which suits prose."));
    m_layoutBox->setCurrentIndex(
      qBound(0, config.ocrTextLayout(), m_layoutBox->count() - 1));
    connect(
      m_layoutBox,
      static_cast<void (QComboBox::*)(int)>(&QComboBox::currentIndexChanged),
      this,
      &OcrResultsWindow::onLayoutChanged);
    optionsLayout->addWidget(m_layoutBox);
    optionsLayout->addStretch();
    layout->addLayout(optionsLayout);

    auto* statusLayout = new QHBoxLayout();
    m_spinner = new LoadSpinner(this);
    m_spinner->setColor(config.uiColor());
    m_spinner->hide();
    statusLayout->addWidget(m_spinner);
    m_statusLabel = new QLabel(this);
    m_statusLabel->setWordWrap(true);
    m_statusLabel->hide();
    statusLayout->addWidget(m_statusLabel, 1);
    layout->addLayout(statusLayout);

    m_textEdit = new QPlainTextEdit(this);
    layout->addWidget(m_textEdit, 1);

    auto* buttonLayout = new QHBoxLayout();
    buttonLayout->addStretch();
    m_tryHarderButton = new QPushButton(tr("Try harder"), this);
    m_tryHarderButton->setToolTip(
      tr("Recognize again with every strategy the tool has, including "
         "thresholding and a larger upscale. Slower."));
    // Nothing to improve on until a disappointing result exists
    m_tryHarderButton->setEnabled(false);
    connect(m_tryHarderButton,
            &QPushButton::clicked,
            this,
            &OcrResultsWindow::retryHarder);
    buttonLayout->addWidget(m_tryHarderButton);
    m_copyButton = new QPushButton(tr("Copy all"), this);
    connect(
      m_copyButton, &QPushButton::clicked, this, &OcrResultsWindow::copyAll);
    buttonLayout->addWidget(m_copyButton);
    layout->addLayout(buttonLayout);

    // Busy indicator appears only when recognition is noticeably slow
    m_busyDelay = new QTimer(this);
    m_busyDelay->setSingleShot(true);
    m_busyDelay->setInterval(200);
    connect(m_busyDelay, &QTimer::timeout, this, [this]() {
        m_statusLabel->setText(tr("Recognizing text…"));
        m_statusLabel->show();
        m_spinner->show();
        m_spinner->start();
    });

    auto* probe = new OcrLanguageProbe();
    connect(probe,
            &OcrLanguageProbe::finished,
            this,
            &OcrResultsWindow::onLanguagesProbed);
    startWorker(probe, &OcrLanguageProbe::finished);

    startRecognition();
}

void OcrResultsWindow::copyAll()
{
    QApplication::clipboard()->setText(m_textEdit->toPlainText());
}

void OcrResultsWindow::retryHarder()
{
    startRecognition(OcrEffort::High);
}

void OcrResultsWindow::onLanguageChanged(int index)
{
    Q_UNUSED(index)
    m_language = m_languageBox->currentData().toString();
    ConfigHandler().setOcrLanguage(m_language);
    startRecognition();
}

void OcrResultsWindow::onLayoutChanged(int index)
{
    Q_UNUSED(index)
    ConfigHandler().setOcrTextLayout(m_layoutBox->currentIndex());
    refreshText();
}

void OcrResultsWindow::onLanguagesProbed(const QStringList& languages)
{
    // With one language there is nothing to choose, so the row stays hidden
    if (languages.size() < 2) {
        return;
    }

    // Populating must not look like a user choice and restart recognition
    const QSignalBlocker blocker(m_languageBox);
    m_languageBox->clear();
    m_languageBox->addItem(tr("System default"), QString());
    for (const QString& tag : languages) {
        const QString name = QLocale(tag).nativeLanguageName();
        m_languageBox->addItem(name.isEmpty() ? tag : name, tag);
    }

    const int saved = m_languageBox->findData(m_language);
    // A language that is no longer installed falls back to the default
    m_languageBox->setCurrentIndex(saved >= 0 ? saved : 0);
    if (saved < 0) {
        m_language.clear();
    }

    m_languageRow->show();
}

void OcrResultsWindow::startRecognition(OcrEffort effort)
{
    const quint64 runId = ++m_runId;
    m_statusLabel->hide();
    m_textEdit->setPlainText(QString());
    m_hasResult = false;
    m_tryHarderButton->setEnabled(false);
    m_busyDelay->start();

    // The worker communicates only through a queued connection, which Qt
    // severs safely if this window is closed mid-recognition; the thread
    // then finishes on its own and deletes itself
    auto* worker = new OcrWorker(
      m_capture.toImage(), m_screenRect, m_language, runId, effort);
    connect(
      worker, &OcrWorker::finished, this, &OcrResultsWindow::onWorkerFinished);
    startWorker(worker, &OcrWorker::finished);
}

void OcrResultsWindow::onWorkerFinished(const OcrResult& result, quint64 runId)
{
    // A language change may have started a newer run; ignore stale results
    if (runId != m_runId) {
        return;
    }

    m_busyDelay->stop();
    m_spinner->stop();
    m_spinner->hide();
    m_statusLabel->hide();

    m_result = result;
    m_hasResult = true;

    // Once there is a result, what the user needs to know is where it came
    // from — advice about framing the capture no longer applies to it
    const bool exact = result.source == OcrResult::Source::Exact;
    m_hintLabel->setText(exact
                           ? tr("Exact text, read from the window itself.")
                           : tr("Recognized from the captured image. Capture "
                                "close up and unblurred for the best result."));
    m_hintLabel->setToolTip(result.diagnostics);
    m_statusLabel->setToolTip(result.diagnostics);

    // Exact text has no recognition error in it, so there is nothing a
    // harder attempt could improve
    m_tryHarderButton->setEnabled(!exact);

    refreshText();
    if (result.status != OcrResult::Status::Ok) {
        showStatus(result);
    }
}

void OcrResultsWindow::refreshText()
{
    if (!m_hasResult || m_result.status != OcrResult::Status::Ok) {
        return;
    }
    const auto layout = static_cast<OcrTextLayout>(m_layoutBox->currentIndex());

    // An engine that reports text without boxes has no layout to rebuild
    m_textEdit->setPlainText(m_result.lines.isEmpty()
                               ? m_result.fullText
                               : ocrAssembleText(m_result.lines, layout));
}

void OcrResultsWindow::showStatus(const OcrResult& result)
{
    switch (result.status) {
        case OcrResult::Status::Ok:
            return;
        case OcrResult::Status::NoTextFound:
            m_statusLabel->setText(tr("No text was found in the selection."));
            break;
        case OcrResult::Status::NoLanguageInstalled:
            m_statusLabel->setText(
              tr("No OCR language is installed. Add one under Settings > "
                 "Time & Language > Language & region > Add a language; "
                 "language packs include text recognition."));
            break;
        case OcrResult::Status::Unsupported:
            m_statusLabel->setText(
              tr("Text recognition is not supported on this platform."));
            break;
        case OcrResult::Status::EngineError:
            m_statusLabel->setText(
              result.errorMessage.isEmpty()
                ? tr("Text recognition failed.")
                : tr("Text recognition failed: %1").arg(result.errorMessage));
            break;
    }
    m_statusLabel->show();
}
