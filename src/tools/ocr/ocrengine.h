// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QImage>
#include <QRectF>
#include <QString>
#include <QStringList>
#include <QVector>

struct OcrWord
{
    QString text;
    // Bounding box in the coordinates of the (preprocessed) image that was
    // passed to OcrEngine::recognize()
    QRectF boundingBox;
};

struct OcrLine
{
    QString text;
    // Union of the word boxes, in the same coordinates as OcrWord
    QRectF boundingBox;
    // Word boxes are what make glyph height measurable and the reading order
    // reconstructible; a line box alone cannot distinguish a wide heading
    // from a column of body text
    QVector<OcrWord> words;
};

struct OcrResult
{
    enum class Status
    {
        Ok,
        NoTextFound,
        NoLanguageInstalled,
        Unsupported,
        EngineError,
    };

    QString fullText;
    QVector<OcrLine> lines;
    Status status{ Status::NoTextFound };
    // Engine-specific detail for EngineError (e.g. an HRESULT message).
    // User-facing wording for each status is built by the UI, so that
    // strings can be wrapped in tr() there.
    QString errorMessage;
};

/**
 * @brief Platform-independent interface to an OCR engine.
 *
 * recognize() may block for a while; callers are expected to run it off the
 * GUI thread. Implementations must be stateless or internally synchronized
 * so that recognize() is safe to call from a non-GUI thread.
 */
class OcrEngine
{
public:
    virtual ~OcrEngine() = default;

    // True when this platform provides a working OCR implementation
    virtual bool isAvailable() const = 0;
    // BCP-47 tags of the languages the OS can OCR, empty when none
    virtual QStringList availableLanguages() const = 0;
    // Run OCR on `image`. `language` is a BCP-47 tag from
    // availableLanguages(), or empty for the system default.
    virtual OcrResult recognize(const QImage& image,
                                const QString& language = QString()) = 0;
    // Largest width/height the engine accepts; preprocessing uses it to cap
    // scaling. 0 means unlimited.
    virtual int maxImageDimension() const { return 0; }

    // Create the engine for the current platform. Caller takes ownership.
    static OcrEngine* create();
};
