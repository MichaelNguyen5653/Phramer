// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrpipeline.h"

#include "tools/ocr/ocrpreprocess.h"
#include "tools/ocr/ocrscore.h"
#include "tools/ocr/ocrtiling.h"

#include <QStringList>

namespace {

// The first pass found nothing at all, which for a screenshot almost always
// means the glyphs are too small to resolve rather than that there is no
// text, so retry well upscaled before giving up
constexpr qreal EmptyResultRetryScale = 4.0;

// Roughly a handful of words. Below this the result is a failure worth
// another attempt; above it the extra engine round is not worth the wait.
constexpr int MinAcceptableScore = 40;

// High effort never magnifies less than this, whatever the first pass
// measured -- the user asking for another try is evidence the measurement
// was not to be trusted
constexpr qreal HighEffortMinScale = 2.0;

struct Pass
{
    OcrResult result;
    int score = 0;
};

/// Rebuild fullText from the lines, so a caller that reads it directly sees
/// the same text the lines describe
void refreshFullText(OcrResult& result)
{
    QStringList texts;
    for (const OcrLine& line : result.lines) {
        texts.append(line.text);
    }
    result.fullText = texts.join(QLatin1Char('\n'));
}

/**
 * Recognize one already-scaled image, splitting it when the engine will not
 * take it whole and merging what the pieces found.
 */
OcrResult recognizeTiled(const QImage& image,
                         const OcrRecognizeFn& recognize,
                         const QString& language,
                         int maxDimension,
                         int& tileCount)
{
    const QVector<QRect> tiles = ocrPlanTiles(image.size(), maxDimension);
    tileCount = tiles.size();
    if (tiles.size() <= 1) {
        return recognize(image, language);
    }

    OcrResult merged;
    QString firstError;
    int failures = 0;

    for (const QRect& tile : tiles) {
        const OcrResult part = recognize(image.copy(tile), language);
        if (part.status == OcrResult::Status::NoLanguageInstalled ||
            part.status == OcrResult::Status::Unsupported) {
            // No other tile can do better; stop paying for them
            return part;
        }
        if (part.status == OcrResult::Status::EngineError) {
            ++failures;
            if (firstError.isEmpty()) {
                firstError = part.errorMessage;
            }
            continue;
        }
        merged.lines += ocrRemapLines(part.lines, tile.topLeft());
    }

    if (failures == tiles.size()) {
        merged.status = OcrResult::Status::EngineError;
        merged.errorMessage = firstError;
        return merged;
    }

    merged.lines = ocrMergeTiledLines(merged.lines);
    merged.status = merged.lines.isEmpty() ? OcrResult::Status::NoTextFound
                                           : OcrResult::Status::Ok;
    refreshFullText(merged);
    return merged;
}

QString describePass(const QString& name, qreal scale, int tiles, int score)
{
    return QStringLiteral("pass %1: scale %2, %3 tile(s), score %4")
      .arg(name)
      .arg(scale, 0, 'f', 2)
      .arg(tiles)
      .arg(score);
}

} // namespace

OcrResult ocrRunPipeline(const QImage& capture,
                         const OcrRecognizeFn& recognize,
                         const QString& language,
                         int maxDimension,
                         OcrEffort effort)
{
    QStringList notes;
    notes.append(QStringLiteral("capture %1x%2")
                   .arg(capture.width())
                   .arg(capture.height()));

    const QImage normalized = ocrNormalizeImage(capture);

    // Pass A exists to measure, not to win: nothing about a screenshot says
    // how tall its glyphs are until an engine has put boxes around some
    int tiles = 0;
    Pass best;
    best.result = recognizeTiled(ocrScaleImage(normalized, 1.0, maxDimension),
                                 recognize,
                                 language,
                                 maxDimension,
                                 tiles);
    best.score = ocrScoreResult(best.result);
    notes.append(describePass(QStringLiteral("A"), 1.0, tiles, best.score));

    if (best.result.status == OcrResult::Status::NoLanguageInstalled ||
        best.result.status == OcrResult::Status::Unsupported) {
        best.result.diagnostics = notes.join(QLatin1Char('\n'));
        return best.result;
    }

    const qreal measured = best.result.lines.isEmpty()
                             ? EmptyResultRetryScale
                             : ocrIdealScale(best.result.lines);
    qreal scale =
      effort == OcrEffort::High ? qMax(measured, HighEffortMinScale) : measured;
    // Tiling, not clamping, is what keeps the engine within its limit here --
    // clamping is the defect this pipeline exists to fix -- but an unbounded
    // upscale would still turn into an unbounded number of round trips
    scale = ocrScaleWithinTileBudget(normalized.size(), scale, maxDimension);

    QImage scaled;
    if (qAbs(scale - 1.0) > 0.05) {
        // Scaling the untouched image rather than pass A's input avoids
        // compounding that pass's interpolation loss
        scaled = ocrScaleImage(normalized, scale, 0);
        Pass passB;
        passB.result =
          recognizeTiled(scaled, recognize, language, maxDimension, tiles);
        passB.score = ocrScoreResult(passB.result);
        notes.append(
          describePass(QStringLiteral("B"), scale, tiles, passB.score));
        if (passB.score > best.score) {
            best = passB;
        }
    }

    if (effort == OcrEffort::High || best.score < MinAcceptableScore) {
        if (scaled.isNull()) {
            scaled = ocrScaleImage(normalized, scale, 0);
        }
        Pass passC;
        passC.result = recognizeTiled(
          ocrBinarize(scaled), recognize, language, maxDimension, tiles);
        passC.score = ocrScoreResult(passC.result);
        notes.append(describePass(
          QStringLiteral("C (binarized)"), scale, tiles, passC.score));
        if (passC.score > best.score) {
            best = passC;
        }
    }

    notes.append(QStringLiteral("kept score %1").arg(best.score));
    refreshFullText(best.result);
    best.result.diagnostics = notes.join(QLatin1Char('\n'));
    return best.result;
}
