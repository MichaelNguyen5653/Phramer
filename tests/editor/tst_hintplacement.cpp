// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/hintplacement.h"

#include <QtTest>

using namespace HintPlacement;

/**
 * @brief Candidate order and rejection for the capture overlay's hint.
 *
 * The hint is painted into CaptureWidget rather than being a widget, so there
 * is no layout to fall back on: if this arithmetic is wrong the hint silently
 * lands off-screen or under a button.
 */
class HintPlacementTest : public QObject
{
    Q_OBJECT

private slots:
    void sitsAboveTheSelectionByDefault();
    void centresOnTheSelection();
    void fallsBelowWhenThereIsNoRoomAbove();
    void fallsInsideWhenTheSelectionFillsTheScreen();
    void avoidsAnObstacleInTheFirstSlot();
    void returnsNothingWhenEveryCandidateIsRejected();
    void clampsToTheScreenHorizontally();
};

namespace {
constexpr int Gap8 = 8;
const QSize Pill(200, 24);
const QRect Screen(0, 0, 1920, 1080);
} // namespace

void HintPlacementTest::sitsAboveTheSelectionByDefault()
{
    const QRect selection(700, 400, 400, 300);
    const QRect hint = place(Pill, selection, Screen, {}, Gap8);

    // The tool buttons default to below the selection, so above is tried
    // first. Stated on the exclusive edge because QRect::bottom() is the last
    // row inside the rectangle, not the first row outside it.
    QCOMPARE(hint.bottom() + 1, selection.top() - Gap8);
    QCOMPARE(hint.size(), Pill);
}

void HintPlacementTest::centresOnTheSelection()
{
    const QRect selection(700, 400, 400, 300);
    const QRect hint = place(Pill, selection, Screen, {}, Gap8);
    // Exact centring is impossible whenever the two widths disagree in parity
    QVERIFY(qAbs(hint.center().x() - selection.center().x()) <= 1);
}

void HintPlacementTest::fallsBelowWhenThereIsNoRoomAbove()
{
    // Selection hard against the top of the screen
    const QRect selection(700, 0, 400, 300);
    const QRect hint = place(Pill, selection, Screen, {}, Gap8);
    QCOMPARE(hint.top(), selection.bottom() + 1 + Gap8);
}

void HintPlacementTest::fallsInsideWhenTheSelectionFillsTheScreen()
{
    const QRect hint = place(Pill, Screen, Screen, {}, Gap8);
    // Last resort: inside the selection, against its bottom edge
    QCOMPARE(hint.bottom() + 1, Screen.bottom() + 1 - Gap8);
    QVERIFY(Screen.contains(hint));
}

void HintPlacementTest::avoidsAnObstacleInTheFirstSlot()
{
    const QRect selection(700, 400, 400, 300);
    // A button strip occupying the whole band above the selection
    const QVector<QRect> obstacles{ QRect(600, 340, 600, 60) };
    const QRect hint = place(Pill, selection, Screen, obstacles, Gap8);
    QCOMPARE(hint.top(), selection.bottom() + 1 + Gap8);
}

void HintPlacementTest::returnsNothingWhenEveryCandidateIsRejected()
{
    // Buttons above, below and across the selection's bottom edge
    const QRect selection(700, 400, 400, 300);
    const QVector<QRect> obstacles{ QRect(0, 0, 1920, 1080) };
    QVERIFY(place(Pill, selection, Screen, obstacles, Gap8).isNull());
}

void HintPlacementTest::clampsToTheScreenHorizontally()
{
    // Selection at the far left: centring would push the pill off-screen
    const QRect selection(0, 400, 40, 300);
    const QRect hint = place(Pill, selection, Screen, {}, Gap8);
    QCOMPARE(hint.left(), Screen.left());
    QVERIFY(Screen.contains(hint));
}

QTEST_MAIN(HintPlacementTest)
#include "tst_hintplacement.moc"
