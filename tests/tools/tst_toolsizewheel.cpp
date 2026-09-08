// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/toolsizewheel.h"

#include <QtTest>

class TestToolSizeWheel : public QObject
{
    Q_OBJECT

private slots:
    void wheelNotchesStepOnce();
    void oversizedNotchesStillStepOnce();
    void thresholdBelongsToTheWheelBranch();
    void touchpadIsRateLimited();
    void touchpadResumesAfterTheWindow();
    void touchpadWithoutVerticalMovementStepsNothing();
};

// A mouse wheel reports a full notch, which is one size step in that direction
void TestToolSizeWheel::wheelNotchesStepOnce()
{
    qint64 last = 0;
    const ToolSizeWheel::Step up = ToolSizeWheel::evaluate(120, 1000, last);
    QVERIFY(up.accepted);
    QVERIFY(up.fromWheel);
    QCOMPARE(up.delta, 1);

    const ToolSizeWheel::Step down = ToolSizeWheel::evaluate(-120, 1000, last);
    QVERIFY(down.accepted);
    QCOMPARE(down.delta, -1);

    // A wheel event is never rate limited, so it must not disturb the
    // touchpad's timestamp
    QCOMPARE(last, 0);
}

void TestToolSizeWheel::oversizedNotchesStillStepOnce()
{
    qint64 last = 0;
    // A fast flick or a high-resolution wheel can report several notches at
    // once; the size must not jump by that much
    QCOMPARE(ToolSizeWheel::evaluate(600, 1000, last).delta, 1);
    QCOMPARE(ToolSizeWheel::evaluate(-600, 1000, last).delta, -1);
}

void TestToolSizeWheel::thresholdBelongsToTheWheelBranch()
{
    qint64 last = 0;
    const ToolSizeWheel::Step atThreshold =
      ToolSizeWheel::evaluate(60, 1000, last);
    QVERIFY(atThreshold.fromWheel);
    QCOMPARE(atThreshold.delta, 1);

    const ToolSizeWheel::Step below = ToolSizeWheel::evaluate(59, 1000, last);
    QVERIFY(!below.fromWheel);
}

// Touchpads emit a stream of small deltas. Without rate limiting every one of
// them would change the size and the tool would be unusable.
void TestToolSizeWheel::touchpadIsRateLimited()
{
    qint64 last = 0;
    const ToolSizeWheel::Step first = ToolSizeWheel::evaluate(5, 1000, last);
    QVERIFY(first.accepted);
    QVERIFY(!first.fromWheel);
    QCOMPARE(first.delta, 1);
    QCOMPARE(last, 1000);

    const ToolSizeWheel::Step tooSoon = ToolSizeWheel::evaluate(5, 1150, last);
    QVERIFY(!tooSoon.accepted);
    QCOMPARE(tooSoon.delta, 0);
    // A rejected event must not extend the window, or a fast stream would
    // never let a step through
    QCOMPARE(last, 1000);
}

void TestToolSizeWheel::touchpadResumesAfterTheWindow()
{
    qint64 last = 1000;
    QVERIFY(!ToolSizeWheel::evaluate(5, 1200, last).accepted);
    const ToolSizeWheel::Step later = ToolSizeWheel::evaluate(5, 1201, last);
    QVERIFY(later.accepted);
    QCOMPARE(later.delta, 1);
    QCOMPARE(last, 1201);

    qint64 downLast = 0;
    QCOMPARE(ToolSizeWheel::evaluate(-5, 5000, downLast).delta, -1);
}

void TestToolSizeWheel::touchpadWithoutVerticalMovementStepsNothing()
{
    qint64 last = 0;
    const ToolSizeWheel::Step flat = ToolSizeWheel::evaluate(0, 1000, last);
    QVERIFY(flat.accepted);
    QCOMPARE(flat.delta, 0);
}

QTEST_APPLESS_MAIN(TestToolSizeWheel)
#include "tst_toolsizewheel.moc"
