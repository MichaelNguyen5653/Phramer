// SPDX-License-Identifier: GPL-3.0-or-later

#include "tools/ocr/ocrlayout.h"
#include "tools/ocr/ocrpipeline.h"
#include "tools/ocr/ocrpreprocess.h"
#include "tools/ocr/ocrscore.h"
#include "tools/ocr/ocrtiling.h"

#include <QTest>

namespace {

OcrLine makeLine(const QString& text, const QRectF& box)
{
    OcrLine line;
    line.text = text;
    line.boundingBox = box;
    // One word spanning the line is enough for the height measurements; the
    // layout code only ever reads the line box
    OcrWord word;
    word.text = text;
    word.boundingBox = box;
    line.words.append(word);
    return line;
}

QVector<OcrLine> linesOfHeight(int count, qreal height)
{
    QVector<OcrLine> lines;
    for (int i = 0; i < count; ++i) {
        lines.append(
          makeLine(QStringLiteral("x"), QRectF(0, i * height * 2, 50, height)));
    }
    return lines;
}

QStringList textsOf(const QVector<OcrLine>& lines)
{
    QStringList out;
    for (const OcrLine& line : lines) {
        out.append(line.text);
    }
    return out;
}

OcrLine makeWordLine(const QStringList& words, qreal top)
{
    OcrLine line;
    line.text = words.join(QLatin1Char(' '));
    qreal x = 0;
    for (const QString& word : words) {
        OcrWord w;
        w.text = word;
        // 10px per character is arbitrary but consistent; only the text
        // lengths matter to scoring
        w.boundingBox = QRectF(x, top, word.size() * 10.0, 20.0);
        x += word.size() * 10.0 + 10.0;
        line.words.append(w);
    }
    line.boundingBox = QRectF(0, top, x, 20.0);
    return line;
}

OcrResult okResult(const QVector<OcrLine>& lines)
{
    OcrResult result;
    result.lines = lines;
    result.status = OcrResult::Status::Ok;
    return result;
}

/// Whether every pixel of `size` falls inside at least one tile. Sampled on
/// a coarse grid plus the four corners: a gap large enough to lose a line of
/// text cannot hide between samples 13px apart.
bool tilesCoverEverything(const QSize& size, const QVector<QRect>& tiles)
{
    QVector<QPoint> samples{ QPoint(0, 0),
                             QPoint(size.width() - 1, 0),
                             QPoint(0, size.height() - 1),
                             QPoint(size.width() - 1, size.height() - 1) };
    for (int y = 0; y < size.height(); y += 13) {
        for (int x = 0; x < size.width(); x += 13) {
            samples.append(QPoint(x, y));
        }
    }
    for (const QPoint& point : samples) {
        bool covered = false;
        for (const QRect& tile : tiles) {
            if (tile.contains(point)) {
                covered = true;
                break;
            }
        }
        if (!covered) {
            return false;
        }
    }
    return true;
}

/// A 100x100 image whose top `darkRows` rows are `dark` and the rest `light`
QImage twoTonePage(int darkRows, int dark, int light)
{
    QImage image(100, 100, QImage::Format_RGBA8888);
    image.fill(QColor(light, light, light));
    for (int y = 0; y < darkRows; ++y) {
        for (int x = 0; x < 100; ++x) {
            image.setPixelColor(x, y, QColor(dark, dark, dark));
        }
    }
    return image;
}

/**
 * Records every image it is handed and answers from a script, so the
 * pipeline's choices can be tested with no engine present. `reply` sees the
 * call index and the image it was given.
 */
struct FakeEngine
{
    QVector<QSize> seen;
    std::function<OcrResult(int index, const QImage& image)> reply;

    OcrRecognizeFn fn()
    {
        return [this](const QImage& image, const QString&) {
            const int index = seen.size();
            seen.append(image.size());
            return reply(index, image);
        };
    }
};

/// A result whose score is roughly `words` * 7
OcrResult scriptedResult(int words, qreal glyphHeight)
{
    QVector<OcrLine> lines;
    for (int i = 0; i < words; ++i) {
        OcrLine line;
        line.text = QStringLiteral("wordy");
        OcrWord word;
        word.text = line.text;
        word.boundingBox = QRectF(0, i * glyphHeight * 2, 50, glyphHeight);
        line.boundingBox = word.boundingBox;
        line.words.append(word);
        lines.append(line);
    }
    return okResult(lines);
}

QImage blankCapture(int width, int height)
{
    QImage image(width, height, QImage::Format_RGBA8888);
    image.fill(Qt::white);
    return image;
}

} // namespace

class OcrTests : public QObject
{
    Q_OBJECT

private slots:
    // --- ocrPadImage -----------------------------------------------------

    void padsImagesBelowTheMinimum()
    {
        QImage small(20, 10, QImage::Format_RGBA8888);
        small.fill(Qt::white);
        small.setPixelColor(0, 0, Qt::red);

        const QImage padded = ocrPadImage(small);

        QVERIFY(padded.width() >= 64);
        QVERIFY(padded.height() >= 64);
        // The border takes the top-left pixel so it reads as background
        QCOMPARE(padded.pixelColor(0, 0), QColor(Qt::red));
        // The original is blitted unscaled at the border offset
        QCOMPARE(padded.pixelColor(8 + 10, 8 + 5), QColor(Qt::white));
    }

    void leavesLargeImagesAlone()
    {
        QImage large(200, 200, QImage::Format_RGBA8888);
        large.fill(Qt::white);

        QCOMPARE(ocrPadImage(large).size(), QSize(200, 200));
    }

    void padsWhenOnlyOneSideIsSmall()
    {
        QImage wide(400, 12, QImage::Format_RGBA8888);
        wide.fill(Qt::white);

        const QImage padded = ocrPadImage(wide);

        QCOMPARE(padded.width(), 400 + 16);
        QVERIFY(padded.height() >= 64);
    }

    // --- ocrIdealScale ---------------------------------------------------

    void scalesSmallGlyphsUpToTheIdealHeight()
    {
        QCOMPARE(ocrIdealScale(linesOfHeight(5, 10.0)), 4.0);
    }

    void leavesIdealGlyphsAlone()
    {
        QCOMPARE(ocrIdealScale(linesOfHeight(5, 40.0)), 1.0);
    }

    void treatsNoMeasurementAsNoChange()
    {
        QCOMPARE(ocrIdealScale(QVector<OcrLine>{}), 1.0);
        QCOMPARE(ocrIdealScale(QVector<OcrLine>{
                   makeLine(QStringLiteral("x"), QRectF(0, 0, 10, 0)) }),
                 1.0);
    }

    void ignoresAnOutlierGlyphHeight()
    {
        // A single misdetected fragment must not decide the whole page's
        // scale; the median is what protects against that
        QVector<OcrLine> lines = linesOfHeight(5, 20.0);
        lines.append(makeLine(QStringLiteral("!"), QRectF(0, 500, 4, 2)));

        QCOMPARE(ocrIdealScale(lines), 2.0);
    }

    void clampsExtremeScales()
    {
        QCOMPARE(ocrIdealScale(linesOfHeight(5, 1.0)), 6.0);
        QCOMPARE(ocrIdealScale(linesOfHeight(5, 400.0)), 0.5);
    }

    void fallsBackToLineBoxesWhenThereAreNoWords()
    {
        OcrLine line;
        line.text = QStringLiteral("x");
        line.boundingBox = QRectF(0, 0, 100, 20);

        QCOMPARE(ocrIdealScale(QVector<OcrLine>{ line }), 2.0);
    }

    // --- ocrScaleImage ---------------------------------------------------

    void scalesAndKeepsTheFormat()
    {
        QImage source(100, 50, QImage::Format_ARGB32);
        source.fill(Qt::white);

        const QImage scaled = ocrScaleImage(source, 2.0);

        QCOMPARE(scaled.size(), QSize(200, 100));
        QCOMPARE(scaled.format(), QImage::Format_RGBA8888);
    }

    void neverExceedsTheEngineLimit()
    {
        QImage source(100, 50, QImage::Format_RGBA8888);
        source.fill(Qt::white);

        const QImage scaled = ocrScaleImage(source, 4.0, 150);

        QVERIFY(scaled.width() <= 150);
        QVERIFY(scaled.height() <= 150);
        // Aspect ratio survives the clamp
        QCOMPARE(scaled.width(), scaled.height() * 2);
    }

    void shrinksImagesAlreadyOverTheLimit()
    {
        QImage source(400, 100, QImage::Format_RGBA8888);
        source.fill(Qt::white);

        QCOMPARE(ocrScaleImage(source, 1.0, 200).size(), QSize(200, 50));
    }

    // --- ocrIsDarkBackground ---------------------------------------------

    void aDarkTerminalIsDark()
    {
        QVERIFY(ocrIsDarkBackground(twoTonePage(90, 18, 255)));
    }

    void aLightPageIsNotDark()
    {
        QVERIFY(!ocrIsDarkBackground(twoTonePage(10, 0, 255)));
    }

    void aMostlyDarkPageWithABrightImageIsDark()
    {
        // Mean luminance here is about 131, so a single-mean test calls this
        // light and leaves a dark page uninverted. The dominant tone and the
        // median both say otherwise.
        QVERIFY(ocrIsDarkBackground(twoTonePage(55, 30, 255)));
    }

    void anEvenlyMidTonedPageIsNotDark()
    {
        // Neither condition holds: the dominant tone is well above the
        // bottom third and the median is not low
        QVERIFY(!ocrIsDarkBackground(twoTonePage(50, 120, 200)));
    }

    // --- ocrBinarize -----------------------------------------------------

    void binarizingLeavesOnlyTwoTones()
    {
        // A ramp stands in for antialiased glyph edges
        QImage ramp(256, 8, QImage::Format_RGBA8888);
        for (int x = 0; x < 256; ++x) {
            for (int y = 0; y < 8; ++y) {
                ramp.setPixelColor(x, y, QColor(x, x, x));
            }
        }

        const QImage flat = ocrBinarize(ramp);

        QCOMPARE(flat.size(), ramp.size());
        for (int x = 0; x < 256; ++x) {
            const int value = flat.pixelColor(x, 0).red();
            QVERIFY(value == 0 || value == 255);
        }
        QCOMPARE(flat.pixelColor(0, 0), QColor(Qt::black));
        QCOMPARE(flat.pixelColor(255, 0), QColor(Qt::white));
    }

    // --- ocrScoreResult --------------------------------------------------

    void moreRecognizedTextScoresHigher()
    {
        const OcrResult rich = okResult({ makeWordLine(
          { QStringLiteral("hello"), QStringLiteral("world") }, 0) });
        const OcrResult sparse =
          okResult({ makeWordLine({ QStringLiteral("hi") }, 0) });

        QVERIFY(ocrScoreResult(rich) > ocrScoreResult(sparse));
    }

    void scatteredPunctuationLosesToRealText()
    {
        // A badly scaled pass comes back as stray marks spread over many
        // lines. Line count alone would let that outscore a clean result.
        QVector<OcrLine> junk;
        for (int i = 0; i < 8; ++i) {
            junk.append(makeWordLine({ QStringLiteral(".") }, i * 30.0));
        }
        const OcrResult clean = okResult({ makeWordLine(
          { QStringLiteral("hello"), QStringLiteral("world") }, 0) });

        QVERIFY(ocrScoreResult(clean) > ocrScoreResult(okResult(junk)));
    }

    void aFailedResultScoresNothing()
    {
        OcrResult failed =
          okResult({ makeWordLine({ QStringLiteral("ignored") }, 0) });
        failed.status = OcrResult::Status::EngineError;

        QCOMPARE(ocrScoreResult(failed), 0);
    }

    void scoresLinesThatHaveNoWordBoxes()
    {
        // Line text without word boxes still counts; an engine may report it
        OcrLine line;
        line.text = QStringLiteral("abcd");
        line.boundingBox = QRectF(0, 0, 40, 20);

        QCOMPARE(ocrScoreResult(okResult({ line })), 6);
    }

    // --- ocrPlanTiles ----------------------------------------------------

    void smallImagesAreASingleTile()
    {
        const QVector<QRect> tiles = ocrPlanTiles(QSize(800, 600), 2600);

        QCOMPARE(tiles.size(), 1);
        QCOMPARE(tiles.first(), QRect(0, 0, 800, 600));
    }

    void anUnlimitedEngineIsASingleTile()
    {
        const QVector<QRect> tiles = ocrPlanTiles(QSize(9000, 9000), 0);

        QCOMPARE(tiles.size(), 1);
        QCOMPARE(tiles.first(), QRect(0, 0, 9000, 9000));
    }

    void oversizedImagesAreSplitWithoutGaps()
    {
        const QSize size(5000, 3000);
        const QVector<QRect> tiles = ocrPlanTiles(size, 2000);

        QVERIFY(tiles.size() > 1);
        for (const QRect& tile : tiles) {
            QVERIFY(tile.width() <= 2000);
            QVERIFY(tile.height() <= 2000);
            QVERIFY(QRect(QPoint(0, 0), size).contains(tile));
        }
        QVERIFY(tilesCoverEverything(size, tiles));
    }

    void adjacentTilesOverlap()
    {
        // A line of text sitting on a seam has to be whole in one tile or
        // the merge has nothing to prefer
        const QVector<QRect> tiles = ocrPlanTiles(QSize(4000, 500), 2000);

        QVERIFY(tiles.size() >= 2);
        QVERIFY(tiles[0].right() > tiles[1].left());
    }

    // --- ocrScaleWithinTileBudget ----------------------------------------

    void aScaleThatAlreadyFitsIsUntouched()
    {
        QCOMPARE(ocrScaleWithinTileBudget(QSize(400, 300), 2.0, 2600), 2.0);
    }

    void anExtremeScaleIsReducedToTheTileBudget()
    {
        const QSize size(3840, 2160);
        const qreal scale = ocrScaleWithinTileBudget(size, 6.0, 2600);

        QVERIFY(scale < 6.0);
        const QSize scaled(qRound(size.width() * scale),
                           qRound(size.height() * scale));
        QVERIFY(ocrPlanTiles(scaled, 2600).size() <= OcrMaxTiles);
    }

    // --- ocrRemapLines ---------------------------------------------------

    void remappingTranslatesLineAndWordBoxes()
    {
        const QVector<OcrLine> remapped = ocrRemapLines(
          { makeWordLine({ QStringLiteral("abc") }, 5.0) }, QPointF(100, 200));

        QCOMPARE(remapped.first().boundingBox.left(), 100.0);
        QCOMPARE(remapped.first().boundingBox.top(), 205.0);
        QCOMPARE(remapped.first().words.first().boundingBox.left(), 100.0);
        QCOMPARE(remapped.first().words.first().boundingBox.top(), 205.0);
    }

    // --- ocrMergeTiledLines ----------------------------------------------

    void seamDuplicatesCollapseToTheLongerReading()
    {
        // The same line seen through two overlapping tiles: one tile saw all
        // of it, the other only the part inside its own bounds
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("full sentence here"),
                     QRectF(0, 0, 200, 20)),
            makeLine(QStringLiteral("full sen"), QRectF(0, 0, 190, 20)),
        };

        const QVector<OcrLine> merged = ocrMergeTiledLines(lines);

        QCOMPARE(merged.size(), 1);
        QCOMPARE(merged.first().text, QStringLiteral("full sentence here"));
    }

    void distinctNeighbouringLinesAreNotMerged()
    {
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("first"), QRectF(0, 0, 100, 20)),
            makeLine(QStringLiteral("second"), QRectF(0, 24, 100, 20)),
            makeLine(QStringLiteral("beside"), QRectF(120, 0, 100, 20)),
        };

        QCOMPARE(ocrMergeTiledLines(lines).size(), 3);
    }

    // --- ocrOrderLines ---------------------------------------------------

    void readsColumnsOneAtATime()
    {
        // Two columns separated by a wide gutter, emitted interleaved as the
        // engine tends to for side-by-side windows
        QVector<OcrLine> lines;
        for (int row = 0; row < 3; ++row) {
            lines.append(makeLine(QStringLiteral("L%1").arg(row),
                                  QRectF(0, row * 40, 180, 20)));
            lines.append(makeLine(QStringLiteral("R%1").arg(row),
                                  QRectF(300, row * 40, 180, 20)));
        }

        QCOMPARE(textsOf(ocrOrderLines(lines)),
                 (QStringList{ "L0", "L1", "L2", "R0", "R1", "R2" }));
    }

    void ordersFragmentsOfOneRowLeftToRight()
    {
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("right"), QRectF(200, 0, 80, 20)),
            makeLine(QStringLiteral("left"), QRectF(0, 2, 80, 20)),
            makeLine(QStringLiteral("below"), QRectF(0, 60, 80, 20)),
        };

        QCOMPARE(textsOf(ocrOrderLines(lines)),
                 (QStringList{ "left", "right", "below" }));
    }

    void doesNotSplitOrdinaryProse()
    {
        // A ragged right margin is not a gutter
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("a"), QRectF(0, 0, 400, 20)),
            makeLine(QStringLiteral("b"), QRectF(0, 30, 380, 20)),
            makeLine(QStringLiteral("c"), QRectF(0, 60, 395, 20)),
            makeLine(QStringLiteral("d"), QRectF(0, 90, 210, 20)),
        };

        QCOMPARE(textsOf(ocrOrderLines(lines)),
                 (QStringList{ "a", "b", "c", "d" }));
    }

    void ignoresAGutterWithOnlyAStrayFragmentBesideIt()
    {
        // One line alone on the far side is evidence of an indent or a page
        // number, not of a second column
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("a"), QRectF(0, 0, 180, 20)),
            makeLine(QStringLiteral("b"), QRectF(0, 30, 180, 20)),
            makeLine(QStringLiteral("c"), QRectF(0, 60, 180, 20)),
            makeLine(QStringLiteral("page"), QRectF(400, 90, 40, 20)),
        };

        QCOMPARE(textsOf(ocrOrderLines(lines)),
                 (QStringList{ "a", "b", "c", "page" }));
    }

    // --- ocrAssembleText -------------------------------------------------

    void joinsWrappedParagraphLines()
    {
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("the quick brown"), QRectF(0, 0, 300, 20)),
            makeLine(QStringLiteral("fox jumps over"), QRectF(0, 24, 300, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Join),
                 QStringLiteral("the quick brown fox jumps over"));
    }

    void keepsWidelySpacedLinesApart()
    {
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("heading"), QRectF(0, 0, 300, 20)),
            makeLine(QStringLiteral("body"), QRectF(0, 60, 300, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Join),
                 QStringLiteral("heading\nbody"));
    }

    void keepsDifferentlySizedLinesApart()
    {
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("Title"), QRectF(0, 0, 300, 40)),
            makeLine(QStringLiteral("body"), QRectF(0, 44, 300, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Join),
                 QStringLiteral("Title\nbody"));
    }

    void treatsFragmentsOfOneRowAsOneLine()
    {
        // Two boxes on one row are a single line with a gap in it -- not two
        // lines, and not a paragraph to merge
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("left"), QRectF(0, 0, 80, 20)),
            makeLine(QStringLiteral("right"), QRectF(200, 2, 80, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Join),
                 QStringLiteral("left      right"));
    }

    void neverReadsOneRowAsAWrappedParagraph()
    {
        // Two boxes on one row have a negative gap, which the naive test
        // would read as very tight leading
        QVERIFY(!ocrIsWrappedContinuation(QRectF(0, 0, 80, 20),
                                          QRectF(200, 2, 80, 20)));
    }

    void joiningOffKeepsEveryLineSeparate()
    {
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("the quick brown"), QRectF(0, 0, 300, 20)),
            makeLine(QStringLiteral("fox jumps over"), QRectF(0, 24, 300, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Plain),
                 QStringLiteral("the quick brown\nfox jumps over"));
    }

    void trimsAndDropsEmptyLines()
    {
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("  padded  "), QRectF(0, 0, 300, 20)),
            makeLine(QStringLiteral("   "), QRectF(0, 60, 300, 20)),
            makeLine(QStringLiteral("next"), QRectF(0, 120, 300, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Plain),
                 QStringLiteral("padded\nnext"));
    }

    // --- layout preservation ---------------------------------------------

    void estimatesCharWidthFromWordBoxes()
    {
        // 5 characters spanning 50px is a 10px character
        QCOMPARE(ocrEstimateCharWidth(QVector<OcrLine>{
                   makeLine(QStringLiteral("abcde"), QRectF(0, 0, 50, 20)) }),
                 10.0);
        QCOMPARE(ocrEstimateCharWidth(QVector<OcrLine>{}), 0.0);
    }

    void preservesLeadingIndentation()
    {
        // Two lines of "code", the second indented by four characters
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("if x:"), QRectF(0, 0, 50, 20)),
            makeLine(QStringLiteral("pass"), QRectF(40, 30, 40, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Preserve),
                 QStringLiteral("if x:\n    pass"));
    }

    void preservesGapsBetweenColumns()
    {
        // Two fragments on one row separated by a wide gap: the recognizer
        // reports them separately and the whitespace only exists in the
        // geometry
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("name"), QRectF(0, 0, 40, 20)),
            makeLine(QStringLiteral("value"), QRectF(100, 0, 50, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Preserve),
                 QStringLiteral("name      value"));
    }

    void plainLayoutDropsAlignment()
    {
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("name"), QRectF(0, 0, 40, 20)),
            makeLine(QStringLiteral("value"), QRectF(100, 0, 50, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Plain),
                 QStringLiteral("name value"));
    }

    void joinDropsTheContinuationIndent()
    {
        // A joined paragraph is one line, so the second row's indentation
        // must not reappear as a gap in the middle of it
        QVector<OcrLine> lines{
            makeLine(QStringLiteral("the quick brown"), QRectF(0, 0, 150, 20)),
            makeLine(QStringLiteral("fox jumps"), QRectF(40, 24, 90, 20)),
        };

        QCOMPARE(ocrAssembleText(lines, OcrTextLayout::Join),
                 QStringLiteral("the quick brown fox jumps"));
    }

    // --- ocrRunPipeline --------------------------------------------------

    void aBetterLaterPassWins()
    {
        FakeEngine engine;
        // The first pass measures 10px glyphs, so the second runs at 4x and
        // reads far more
        engine.reply = [](int index, const QImage&) {
            return index == 0 ? scriptedResult(2, 10.0)
                              : scriptedResult(20, 40.0);
        };

        const OcrResult result =
          ocrRunPipeline(blankCapture(400, 300), engine.fn(), QString(), 2600);

        QCOMPARE(result.status, OcrResult::Status::Ok);
        QCOMPARE(result.lines.size(), 20);
    }

    void aWorseLaterPassIsDiscarded()
    {
        FakeEngine engine;
        engine.reply = [](int index, const QImage&) {
            return index == 0 ? scriptedResult(20, 10.0)
                              : scriptedResult(2, 40.0);
        };

        const OcrResult result =
          ocrRunPipeline(blankCapture(400, 300), engine.fn(), QString(), 2600);

        QCOMPARE(result.lines.size(), 20);
    }

    void anEmptyFirstPassTriggersTheUpscaleRetry()
    {
        FakeEngine engine;
        engine.reply = [](int index, const QImage&) {
            return index == 0 ? OcrResult{} : scriptedResult(5, 40.0);
        };

        const OcrResult result =
          ocrRunPipeline(blankCapture(400, 300), engine.fn(), QString(), 2600);

        QVERIFY(engine.seen.size() >= 2);
        const qreal ratio =
          qreal(engine.seen[1].width()) / qreal(engine.seen[0].width());
        QVERIFY2(qAbs(ratio - 4.0) < 0.05,
                 qPrintable(QStringLiteral("ratio was %1").arg(ratio)));
        QCOMPARE(result.lines.size(), 5);
    }

    void anOversizedPassIsTiledRatherThanShrunk()
    {
        FakeEngine engine;
        // Small glyphs, so the second pass wants a large upscale on an image
        // that is already over the limit
        engine.reply = [](int, const QImage&) {
            return scriptedResult(3, 8.0);
        };

        ocrRunPipeline(blankCapture(3000, 800), engine.fn(), QString(), 1000);

        QVERIFY(engine.seen.size() > 1);
        for (const QSize& size : engine.seen) {
            QVERIFY(size.width() <= 1000);
            QVERIFY(size.height() <= 1000);
        }
    }

    void highEffortRunsMorePassesThanNormal()
    {
        FakeEngine normal;
        normal.reply = [](int, const QImage&) {
            return scriptedResult(30, 40.0);
        };
        ocrRunPipeline(blankCapture(400, 300), normal.fn(), QString(), 2600);

        FakeEngine high;
        high.reply = [](int, const QImage&) {
            return scriptedResult(30, 40.0);
        };
        ocrRunPipeline(
          blankCapture(400, 300), high.fn(), QString(), 2600, OcrEffort::High);

        QVERIFY(high.seen.size() > normal.seen.size());
    }

    void everyPassIsDescribedInTheDiagnostics()
    {
        FakeEngine engine;
        engine.reply = [](int, const QImage&) {
            return scriptedResult(30, 40.0);
        };

        const OcrResult result =
          ocrRunPipeline(blankCapture(400, 300), engine.fn(), QString(), 2600);

        QVERIFY(result.diagnostics.contains(QStringLiteral("400x300")));
        QVERIFY(result.diagnostics.contains(QStringLiteral("pass A")));
    }
};

QTEST_GUILESS_MAIN(OcrTests)

#include "tst_ocr.moc"
