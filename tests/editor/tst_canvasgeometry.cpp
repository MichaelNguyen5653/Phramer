// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/canvasgeometry.h"

#include <QtTest>

using namespace CanvasGeometry;

/**
 * @brief The editor's zoom arithmetic.
 *
 * Stepping, clamping and the snap back to actual size can all be wrong without
 * anything crashing, so they live in a free function and are pinned here.
 */
class CanvasGeometryTest : public QObject
{
    Q_OBJECT

private slots:
    void zoomInMultiplies();
    void zoomOutDivides();
    void zoomAccumulatesNotches();
    void zoomSnapsToActualSizeWhenCrossingIt();
    void zoomClampsAtBothEnds();
    void zeroNotchesLeaveZoomAlone();
    void marginIsAShareOfTheLargerSide();
    void marginStaysWithinItsBounds();
    void marginLandsOnWholePhysicalPixels();
    void toPhysicalRoundsOutward();
    void exportIsTheImageWhenNothingReachesPast();
    void exportGrowsToTakeInAnnotations();
    void exportNeverLeavesTheWorkspace();
};

void CanvasGeometryTest::zoomInMultiplies()
{
    QCOMPARE(zoomStep(1.0, 1), 1.25);
}

void CanvasGeometryTest::zoomOutDivides()
{
    QCOMPARE(zoomStep(1.0, -1), 0.8);
}

void CanvasGeometryTest::zoomAccumulatesNotches()
{
    // Two notches compound rather than adding
    QCOMPARE(zoomStep(1.0, 2), 1.5625);
}

void CanvasGeometryTest::zoomSnapsToActualSizeWhenCrossingIt()
{
    // 0.9 * 1.25 == 1.125, which steps over 1.0. Landing on it instead is what
    // lets the user return to actual size with the wheel alone.
    QCOMPARE(zoomStep(0.9, 1), 1.0);
    // 1.1 / 1.25 == 0.88, crossing from the other side
    QCOMPARE(zoomStep(1.1, -1), 1.0);
}

void CanvasGeometryTest::zoomClampsAtBothEnds()
{
    QCOMPARE(zoomStep(MaxZoom, 1), MaxZoom);
    QCOMPARE(zoomStep(MinZoom, -1), MinZoom);
    // A step that would overshoot lands exactly on the limit
    QCOMPARE(zoomStep(MaxZoom / 1.1, 1), MaxZoom);
}

void CanvasGeometryTest::zeroNotchesLeaveZoomAlone()
{
    QCOMPARE(zoomStep(1.7, 0), 1.7);
}

void CanvasGeometryTest::marginIsAShareOfTheLargerSide()
{
    QCOMPARE(workspaceMargin(QSize(1000, 600), 1.0), 400);
    QCOMPARE(workspaceMargin(QSize(600, 1000), 1.0), 400);
}

void CanvasGeometryTest::marginStaysWithinItsBounds()
{
    QCOMPARE(workspaceMargin(QSize(50, 40), 1.0), MinWorkspaceMargin);
    QCOMPARE(workspaceMargin(QSize(8000, 4000), 1.0), MaxWorkspaceMargin);
    // At the upper bound a scale that needs an upward nudge steps down
    // instead of past the bound
    const qreal odd = 7.0 / 6.0; // 800 at 7/6 is 933.3 physical pixels
    const int top = workspaceMargin(QSize(8000, 4000), odd);
    QVERIFY(top <= MaxWorkspaceMargin);
    QVERIFY(qFuzzyCompare(top * odd, std::round(top * odd)));
    QCOMPARE(top, 798);
}

void CanvasGeometryTest::marginLandsOnWholePhysicalPixels()
{
    for (qreal ratio : { 1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0 }) {
        for (int side : { 50, 401, 777, 1003, 3000 }) {
            const int margin = workspaceMargin(QSize(side, side), ratio);
            const qreal physical = margin * ratio;
            QVERIFY2(qFuzzyCompare(physical, std::round(physical)),
                     qPrintable(QStringLiteral("ratio %1 side %2 margin %3")
                                  .arg(ratio)
                                  .arg(side)
                                  .arg(margin)));
            QVERIFY(margin >= MinWorkspaceMargin);
            QVERIFY(margin <= MaxWorkspaceMargin);
        }
    }
}

void CanvasGeometryTest::toPhysicalRoundsOutward()
{
    QCOMPARE(toPhysical(QRect(10, 10, 20, 20), 1.0), QRect(10, 10, 20, 20));
    // 9..29 at 1.25 is 11.25..36.25, which rounds outward to 11..37
    QCOMPARE(toPhysical(QRect(9, 9, 20, 20), 1.25), QRect(11, 11, 26, 26));
    QCOMPARE(toPhysical(QRect(-3, 0, 5, 5), 1.5), QRect(-5, 0, 8, 8));
    QVERIFY(toPhysical(QRect(), 1.25).isNull());
}

void CanvasGeometryTest::exportIsTheImageWhenNothingReachesPast()
{
    const QRect image(200, 200, 700, 400);
    const QRect workspace(0, 0, 1100, 800);
    QCOMPARE(exportRect(image, {}, workspace), image);
    QCOMPARE(exportRect(image, { QRect(250, 250, 50, 50), QRect() }, workspace),
             image);
}

void CanvasGeometryTest::exportGrowsToTakeInAnnotations()
{
    const QRect image(200, 200, 700, 400);
    const QRect workspace(0, 0, 1100, 800);
    // An arrow starting left of the image and a note below it
    QCOMPARE(exportRect(image,
                        { QRect(120, 300, 150, 20), QRect(400, 580, 100, 90) },
                        workspace),
             QRect(QPoint(120, 200), QPoint(899, 669)));
}

void CanvasGeometryTest::exportNeverLeavesTheWorkspace()
{
    const QRect image(200, 200, 700, 400);
    const QRect workspace(0, 0, 1100, 800);
    QCOMPARE(exportRect(image, { QRect(-50, -50, 400, 400) }, workspace),
             QRect(QPoint(0, 0), QPoint(899, 599)));
}

QTEST_MAIN(CanvasGeometryTest)
#include "tst_canvasgeometry.moc"
