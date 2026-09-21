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

QTEST_MAIN(CanvasGeometryTest)
#include "tst_canvasgeometry.moc"
