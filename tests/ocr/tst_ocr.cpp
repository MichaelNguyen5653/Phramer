// SPDX-License-Identifier: GPL-3.0-or-later

#include "tools/ocr/ocrlayout.h"
#include "tools/ocr/ocrpreprocess.h"

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
};

QTEST_GUILESS_MAIN(OcrTests)

#include "tst_ocr.moc"
