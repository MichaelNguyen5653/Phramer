// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/windowsnap.h"

#include <QtTest>

/**
 * @brief Hit-testing and coordinate mapping for window capture.
 *
 * Window rectangles come from DWM in physical screen pixels, while the
 * overlay works in logical pixels relative to its own corner. Getting either
 * step wrong selects a window offset or scaled from the one under the mouse.
 */
class WindowSnapTest : public QObject
{
    Q_OBJECT

private slots:
    void picksTheTopmostWindowUnderThePoint();
    void returnsNothingOverNoWindow();
    void mapsPhysicalToOverlayAtOneHundredPercent();
    void mapsPhysicalToOverlayAtOneTwentyFivePercent();
    void handlesAMonitorAboveThePrimary();
    void clipsAWindowSpanningTwoMonitors();
    void dropsAWindowOnAnotherMonitor();
};

void WindowSnapTest::picksTheTopmostWindowUnderThePoint()
{
    // Topmost first, as EnumWindows reports them
    const QVector<QRect> windows{ QRect(100, 100, 200, 200),
                                  QRect(0, 0, 1000, 800) };
    QCOMPARE(WindowSnap::windowAt(windows, QPoint(150, 150)), windows.at(0));
    QCOMPARE(WindowSnap::windowAt(windows, QPoint(500, 500)), windows.at(1));
}

void WindowSnapTest::returnsNothingOverNoWindow()
{
    const QVector<QRect> windows{ QRect(100, 100, 200, 200) };
    QVERIFY(WindowSnap::windowAt(windows, QPoint(5, 5)).isNull());
}

void WindowSnapTest::mapsPhysicalToOverlayAtOneHundredPercent()
{
    const QRect overlay(0, 0, 1920, 1080);
    const QRect mapped = WindowSnap::toOverlay(
      QRect(100, 50, 800, 600), QPoint(0, 0), 1.0, overlay);
    QCOMPARE(mapped, QRect(100, 50, 800, 600));
}

void WindowSnapTest::mapsPhysicalToOverlayAtOneTwentyFivePercent()
{
    // A 1920x1200 panel at 125% is a 1536x960 overlay
    const QRect overlay(0, 0, 1536, 960);
    const QRect mapped = WindowSnap::toOverlay(
      QRect(125, 250, 1000, 500), QPoint(0, 0), 1.25, overlay);
    QCOMPARE(mapped, QRect(100, 200, 800, 400));
}

void WindowSnapTest::handlesAMonitorAboveThePrimary()
{
    // The overlay's own corner sits at negative physical coordinates
    const QRect overlay(0, 0, 1920, 1080);
    const QRect mapped = WindowSnap::toOverlay(
      QRect(1073, -1000, 400, 300), QPoint(973, -1080), 1.0, overlay);
    QCOMPARE(mapped, QRect(100, 80, 400, 300));
}

void WindowSnapTest::clipsAWindowSpanningTwoMonitors()
{
    const QRect overlay(0, 0, 1920, 1080);
    const QRect mapped = WindowSnap::toOverlay(
      QRect(-200, 100, 600, 300), QPoint(0, 0), 1.0, overlay);
    QCOMPARE(mapped, QRect(0, 100, 400, 300));
}

void WindowSnapTest::dropsAWindowOnAnotherMonitor()
{
    const QRect overlay(0, 0, 1920, 1080);
    QVERIFY(WindowSnap::toOverlay(
              QRect(3000, 100, 600, 300), QPoint(0, 0), 1.0, overlay)
              .isNull());
}

QTEST_MAIN(WindowSnapTest)
#include "tst_windowsnap.moc"
