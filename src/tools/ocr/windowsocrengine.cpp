// SPDX-License-Identifier: GPL-3.0-or-later

#include "windowsocrengine.h"

#ifndef NOMINMAX
#define NOMINMAX
#endif

#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Globalization.h>
#include <winrt/Windows.Graphics.Imaging.h>
#include <winrt/Windows.Media.Ocr.h>
#include <winrt/Windows.Security.Cryptography.h>
#include <winrt/Windows.Storage.Streams.h>

namespace {

namespace WinOcr = winrt::Windows::Media::Ocr;
namespace WinImaging = winrt::Windows::Graphics::Imaging;

QString toQString(const winrt::hstring& str)
{
    return QString::fromWCharArray(str.c_str(), static_cast<int>(str.size()));
}

/**
 * Initializes a COM apartment for the calling thread and releases it again
 * on scope exit. Initialization fails harmlessly when the thread already
 * has an apartment of another type (the Qt GUI thread is STA), in which
 * case the existing one is left untouched. Declare before any WinRT local
 * so those are destroyed first.
 */
struct ApartmentScope
{
    bool owned = false;

    ApartmentScope()
    {
        try {
            winrt::init_apartment(winrt::apartment_type::multi_threaded);
            owned = true;
        } catch (...) {
        }
    }

    ~ApartmentScope()
    {
        if (owned) {
            try {
                winrt::uninit_apartment();
            } catch (...) {
            }
        }
    }
};

WinImaging::SoftwareBitmap toSoftwareBitmap(const QImage& image)
{
    // Format_RGBA8888 rows are contiguous: bytesPerLine == width * 4
    auto buffer = winrt::Windows::Security::Cryptography::CryptographicBuffer::
      CreateFromByteArray(winrt::array_view<uint8_t const>(
        image.constBits(), image.constBits() + image.sizeInBytes()));
    return WinImaging::SoftwareBitmap::CreateCopyFromBuffer(
      buffer,
      WinImaging::BitmapPixelFormat::Rgba8,
      image.width(),
      image.height(),
      WinImaging::BitmapAlphaMode::Ignore);
}

} // namespace

bool WindowsOcrEngine::isAvailable() const
{
    // Windows.Media.Ocr ships with every supported Windows version; having
    // no OCR language installed is reported per-recognition instead
    return true;
}

QStringList WindowsOcrEngine::availableLanguages() const
{
    ApartmentScope apartment;
    QStringList languages;
    try {
        for (const auto& lang :
             WinOcr::OcrEngine::AvailableRecognizerLanguages()) {
            languages.append(toQString(lang.LanguageTag()));
        }
    } catch (...) {
        // Treated as no languages; recognize() will report the actual error
    }
    return languages;
}

int WindowsOcrEngine::maxImageDimension() const
{
    ApartmentScope apartment;
    try {
        return static_cast<int>(WinOcr::OcrEngine::MaxImageDimension());
    } catch (...) {
        return 2600; // documented value for current Windows versions
    }
}

OcrResult WindowsOcrEngine::recognize(const QImage& image,
                                      const QString& language)
{
    OcrResult result;
    if (image.isNull()) {
        result.status = OcrResult::Status::NoTextFound;
        return result;
    }
    ApartmentScope apartment;
    try {
        // Deliberately built per call and never cached: ApartmentScope
        // uninitializes the COM apartment when this scope ends, and a WinRT
        // interface that outlives its apartment is undefined behaviour
        WinOcr::OcrEngine engine{ nullptr };
        if (!language.isEmpty()) {
            winrt::Windows::Globalization::Language lang{ winrt::hstring(
              language.toStdWString()) };
            engine = WinOcr::OcrEngine::TryCreateFromLanguage(lang);
        }
        if (!engine) {
            engine = WinOcr::OcrEngine::TryCreateFromUserProfileLanguages();
        }
        if (!engine) {
            // The user profile languages may lack OCR support even though
            // another installed language pack provides it
            auto available = WinOcr::OcrEngine::AvailableRecognizerLanguages();
            if (available.Size() > 0) {
                engine =
                  WinOcr::OcrEngine::TryCreateFromLanguage(available.GetAt(0));
            }
        }
        if (!engine) {
            result.status = OcrResult::Status::NoLanguageInstalled;
            return result;
        }

        const QImage rgba = image.format() == QImage::Format_RGBA8888
                              ? image
                              : image.convertToFormat(QImage::Format_RGBA8888);
        auto bitmap = toSoftwareBitmap(rgba);
        // A blocking get() is fine here: recognize() is documented to run
        // on a worker thread
        auto ocrResult = engine.RecognizeAsync(bitmap).get();

        QStringList lineTexts;
        for (const auto& line : ocrResult.Lines()) {
            OcrLine ocrLine;
            ocrLine.text = toQString(line.Text());
            QRectF box;
            for (const auto& word : line.Words()) {
                const auto rect = word.BoundingRect();
                OcrWord ocrWord;
                ocrWord.text = toQString(word.Text());
                ocrWord.boundingBox =
                  QRectF(rect.X, rect.Y, rect.Width, rect.Height);
                box = box.united(ocrWord.boundingBox);
                ocrLine.words.append(ocrWord);
            }
            ocrLine.boundingBox = box;
            result.lines.append(ocrLine);
            lineTexts.append(ocrLine.text);
        }
        result.fullText = lineTexts.join(QLatin1Char('\n'));
        result.status = result.lines.isEmpty() ? OcrResult::Status::NoTextFound
                                               : OcrResult::Status::Ok;
    } catch (const winrt::hresult_error& e) {
        result.status = OcrResult::Status::EngineError;
        result.errorMessage = toQString(e.message());
    } catch (const std::exception& e) {
        result.status = OcrResult::Status::EngineError;
        result.errorMessage = QString::fromUtf8(e.what());
    } catch (...) {
        result.status = OcrResult::Status::EngineError;
    }
    return result;
}
