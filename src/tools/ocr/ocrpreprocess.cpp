// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrpreprocess.h"

#include <QPainter>

#include <algorithm>

namespace {

// The engine recognizes glyphs of roughly this height most reliably; both
// smaller and much larger text does measurably worse
constexpr qreal IdealGlyphHeight = 40.0;

// Bounds on the measured scale. Below 1 the image is shrunk, which helps
// oversized text and costs nothing; the upper bound stops a single
// misdetected two-pixel fragment from demanding a huge rescale.
constexpr qreal MinScale = 0.5;
constexpr qreal MaxScale = 6.0;

bool hasDarkBackground(const QImage& image)
{
    // Mean luminance of a small resample is enough to classify; exact
    // statistics are not needed
    const QImage sample =
      image.scaled(32, 32, Qt::IgnoreAspectRatio, Qt::FastTransformation)
        .convertToFormat(QImage::Format_Grayscale8);
    const uchar* bits = sample.constBits();
    const qsizetype size = sample.sizeInBytes();
    if (size <= 0) {
        return false;
    }
    qint64 sum = 0;
    for (qsizetype i = 0; i < size; ++i) {
        sum += bits[i];
    }
    return (sum / size) < 100;
}

} // namespace

QImage ocrPadImage(const QImage& image, int minSide, int border)
{
    if (image.isNull() ||
        (image.width() >= minSide && image.height() >= minSide)) {
        return image;
    }

    const int width = qMax(image.width() + 2 * border, minSide + 2 * border);
    const int height = qMax(image.height() + 2 * border, minSide + 2 * border);

    QImage padded(width, height, image.format());
    padded.fill(image.pixelColor(0, 0));
    QPainter painter(&padded);
    painter.drawImage(border, border, image);
    painter.end();
    return padded;
}

QImage ocrNormalizeImage(const QImage& image)
{
    if (image.isNull()) {
        return image;
    }

    QImage prepared = image.convertToFormat(QImage::Format_RGBA8888);

    // The engine is trained mostly on dark-on-light text; terminals and dark
    // themes recognize much better inverted. Inverting before padding keeps
    // the padded border the same colour as the background it extends.
    if (hasDarkBackground(prepared)) {
        prepared.invertPixels();
    }

    return ocrPadImage(prepared);
}

QImage ocrScaleImage(const QImage& image, qreal scale, int maxDimension)
{
    if (image.isNull()) {
        return image;
    }

    QImage scaled = image;

    if (maxDimension > 0) {
        const int maxSide = qMax(scaled.width(), scaled.height());
        scale = qMin(scale, qreal(maxDimension) / maxSide);
    }

    if (scale < 0.99 || scale > 1.01) {
        const QSize target(qMax(1, qRound(scaled.width() * scale)),
                           qMax(1, qRound(scaled.height() * scale)));
        scaled =
          scaled.scaled(target, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    }

    // Oversized captures must fit within the engine's limit even when no
    // upscaling was asked for
    if (maxDimension > 0 &&
        (scaled.width() > maxDimension || scaled.height() > maxDimension)) {
        scaled = scaled.scaled(maxDimension,
                               maxDimension,
                               Qt::KeepAspectRatio,
                               Qt::SmoothTransformation);
    }

    // scaled() does not necessarily preserve the pixel format
    if (scaled.format() != QImage::Format_RGBA8888) {
        scaled = scaled.convertToFormat(QImage::Format_RGBA8888);
    }
    return scaled;
}

qreal ocrIdealScale(const QVector<OcrLine>& lines)
{
    QVector<qreal> heights;
    for (const OcrLine& line : lines) {
        if (line.words.isEmpty()) {
            // Engines that report no word boxes still give a usable line box
            if (line.boundingBox.height() > 0) {
                heights.append(line.boundingBox.height());
            }
            continue;
        }
        for (const OcrWord& word : line.words) {
            if (word.boundingBox.height() > 0) {
                heights.append(word.boundingBox.height());
            }
        }
    }
    if (heights.isEmpty()) {
        return 1.0;
    }

    // The median ignores the odd oversized heading or misdetected fragment,
    // which an average would let drag the whole page's scale
    std::sort(heights.begin(), heights.end());
    const qreal median = heights[heights.size() / 2];
    if (median <= 0.0) {
        return 1.0;
    }

    return qBound(MinScale, IdealGlyphHeight / median, MaxScale);
}
