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

// Luminance histogram resolution for the dark-ground test. Coarse on
// purpose: it is looking for the dominant tonal region, not a mode.
constexpr int HistogramBuckets = 64;

// The dominant tone must fall in the bottom third of the range, and the
// median pixel must be below this, before the capture is inverted
constexpr int DarkMedianCeiling = 110;

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

bool ocrIsDarkBackground(const QImage& image)
{
    if (image.isNull()) {
        return false;
    }

    // A resample is enough: this is a question about large regions, and the
    // smooth filter keeps a thin bright band from vanishing entirely
    const QImage sample =
      image.scaled(64, 64, Qt::IgnoreAspectRatio, Qt::SmoothTransformation)
        .convertToFormat(QImage::Format_Grayscale8);

    int buckets[HistogramBuckets] = { 0 };
    int total = 0;
    for (int y = 0; y < sample.height(); ++y) {
        const uchar* row = sample.constScanLine(y);
        for (int x = 0; x < sample.width(); ++x) {
            buckets[row[x] * HistogramBuckets / 256] += 1;
            ++total;
        }
    }
    if (total == 0) {
        return false;
    }

    int dominant = 0;
    int seen = 0;
    int median = 0;
    bool haveMedian = false;
    for (int i = 0; i < HistogramBuckets; ++i) {
        if (buckets[i] > buckets[dominant]) {
            dominant = i;
        }
        seen += buckets[i];
        if (!haveMedian && seen * 2 >= total) {
            median = i * 256 / HistogramBuckets;
            haveMedian = true;
        }
    }

    return dominant < HistogramBuckets / 3 && median < DarkMedianCeiling;
}

QImage ocrBinarize(const QImage& image)
{
    if (image.isNull()) {
        return image;
    }

    const QImage grey = image.convertToFormat(QImage::Format_Grayscale8);

    qint64 histogram[256] = { 0 };
    qint64 total = 0;
    for (int y = 0; y < grey.height(); ++y) {
        const uchar* row = grey.constScanLine(y);
        for (int x = 0; x < grey.width(); ++x) {
            histogram[row[x]] += 1;
            ++total;
        }
    }
    if (total == 0) {
        return image;
    }

    // Otsu's method: the threshold that maximizes the variance between the
    // two populations it separates
    qint64 sum = 0;
    for (int i = 0; i < 256; ++i) {
        sum += qint64(i) * histogram[i];
    }
    qint64 belowWeight = 0;
    qint64 belowSum = 0;
    qreal bestVariance = -1.0;
    int threshold = 127;
    for (int i = 0; i < 256; ++i) {
        belowWeight += histogram[i];
        if (belowWeight == 0) {
            continue;
        }
        const qint64 aboveWeight = total - belowWeight;
        if (aboveWeight == 0) {
            break;
        }
        belowSum += qint64(i) * histogram[i];
        const qreal belowMean = qreal(belowSum) / qreal(belowWeight);
        const qreal aboveMean = qreal(sum - belowSum) / qreal(aboveWeight);
        const qreal variance = qreal(belowWeight) * qreal(aboveWeight) *
                               (belowMean - aboveMean) *
                               (belowMean - aboveMean);
        if (variance > bestVariance) {
            bestVariance = variance;
            threshold = i;
        }
    }

    QImage flat(grey.size(), QImage::Format_Grayscale8);
    for (int y = 0; y < grey.height(); ++y) {
        const uchar* source = grey.constScanLine(y);
        uchar* target = flat.scanLine(y);
        for (int x = 0; x < grey.width(); ++x) {
            target[x] = source[x] > threshold ? 255 : 0;
        }
    }
    return flat.convertToFormat(QImage::Format_RGBA8888);
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
    if (ocrIsDarkBackground(prepared)) {
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
