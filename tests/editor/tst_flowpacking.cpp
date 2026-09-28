// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/flowpacking.h"

#include <QtTest>

class TestFlowPacking : public QObject
{
    Q_OBJECT

private slots:
    void emptyTakesNoHeight()
    {
        const FlowPacking::Result r = FlowPacking::pack({}, 500, 2, 2);
        QVERIFY(r.rects.isEmpty());
        QCOMPARE(r.height, 0);
    }

    void fitsOnOneRow()
    {
        const FlowPacking::Result r = FlowPacking::pack(
          { QSize(40, 50), QSize(40, 50), QSize(40, 50) }, 500, 2, 4);
        QCOMPARE(r.rects.at(0), QRect(0, 0, 40, 50));
        QCOMPARE(r.rects.at(1), QRect(42, 0, 40, 50));
        QCOMPARE(r.rects.at(2), QRect(84, 0, 40, 50));
        QCOMPARE(r.height, 50);
    }

    void wrapsWhenTheNextItemWouldCrossTheEdge()
    {
        // 40 + 2 + 40 = 82 fits in 100; the third would end at 124
        const FlowPacking::Result r = FlowPacking::pack(
          { QSize(40, 50), QSize(40, 50), QSize(40, 50) }, 100, 2, 4);
        QCOMPARE(r.rects.at(1).top(), 0);
        QCOMPARE(r.rects.at(2), QRect(0, 54, 40, 50));
        QCOMPARE(r.height, 104);
    }

    void anItemEndingExactlyAtTheEdgeStaysOnTheRow()
    {
        const FlowPacking::Result r =
          FlowPacking::pack({ QSize(49, 20), QSize(49, 20) }, 100, 2, 2);
        QCOMPARE(r.rects.at(1).topLeft(), QPoint(51, 0));
        QCOMPARE(r.height, 20);
    }

    void anItemWiderThanTheRowGetsItsOwnRow()
    {
        const FlowPacking::Result r = FlowPacking::pack(
          { QSize(30, 20), QSize(300, 20), QSize(30, 20) }, 100, 2, 2);
        QCOMPARE(r.rects.at(0).topLeft(), QPoint(0, 0));
        QCOMPARE(r.rects.at(1).topLeft(), QPoint(0, 22));
        QCOMPARE(r.rects.at(2).topLeft(), QPoint(0, 44));
        QCOMPARE(r.height, 64);
    }

    void shorterItemsAreCentredOnTheirRow()
    {
        const FlowPacking::Result r =
          FlowPacking::pack({ QSize(40, 50), QSize(40, 30) }, 500, 2, 2);
        QCOMPARE(r.rects.at(0).top(), 0);
        QCOMPARE(r.rects.at(1).top(), 10);
    }

    void aZeroWidthStillPlacesEveryItem()
    {
        // Before the first layout pass Qt can ask about a width of 0
        const FlowPacking::Result r =
          FlowPacking::pack({ QSize(40, 20), QSize(40, 20) }, 0, 2, 2);
        QCOMPARE(r.rects.size(), 2);
        QCOMPARE(r.rects.at(1).topLeft(), QPoint(0, 22));
    }
};

QTEST_APPLESS_MAIN(TestFlowPacking)
#include "tst_flowpacking.moc"
