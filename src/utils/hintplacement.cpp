// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/hintplacement.h"

namespace HintPlacement {

QRect place(const QSize& hint,
            const QRect& selection,
            const QRect& screen,
            const QVector<QRect>& obstacles,
            int gap)
{
    if (hint.isEmpty() || !screen.isValid()) {
        return {};
    }

    // Horizontal placement is the same wherever the hint ends up vertically:
    // centred on the selection, then pulled back inside the screen. Clamping
    // before the fit test matters, or a selection at the screen edge would
    // reject every candidate for being off-screen.
    const auto centredAt = [&](int top) {
        QRect candidate(QPoint(0, top), hint);
        candidate.moveLeft(selection.center().x() - hint.width() / 2);
        if (candidate.left() < screen.left()) {
            candidate.moveLeft(screen.left());
        }
        if (candidate.right() > screen.right()) {
            candidate.moveRight(screen.right());
        }
        return candidate;
    };

    // bottom() and right() are the last row and column inside a QRect, so
    // every edge below is written as the exclusive one to keep the gap honest.
    const QRect above = centredAt(selection.top() - gap - hint.height());
    const QRect below = centredAt(selection.bottom() + 1 + gap);
    const QRect inside =
      centredAt(selection.bottom() + 1 - gap - hint.height());

    const auto firstHit = [&obstacles](const QRect& candidate) -> const QRect* {
        for (const QRect& obstacle : obstacles) {
            if (candidate.intersects(obstacle)) {
                return &obstacle;
            }
        }
        return nullptr;
    };
    const auto fits = [&](const QRect& candidate) {
        return screen.contains(candidate) && firstHit(candidate) == nullptr;
    };

    // The overlay centres its buttons on the selection edges, so they can
    // block both slots outside it. Step those outward past whatever they
    // hit, one obstacle at a time, until one is clear.
    const auto stepPast = [&](QRect candidate, bool upward) -> QRect {
        // Each step clears one obstacle, so more steps than obstacles means
        // it is going round in circles
        for (int i = 0; i <= obstacles.size(); ++i) {
            if (!screen.contains(candidate)) {
                return {};
            }
            const QRect* hit = firstHit(candidate);
            if (hit == nullptr) {
                return candidate;
            }
            candidate.moveTop(upward ? hit->top() - gap - candidate.height()
                                     : hit->bottom() + 1 + gap);
        }
        return {};
    };

    if (fits(above)) {
        return above;
    }
    if (fits(below)) {
        return below;
    }
    // Stepping comes before the slot inside the selection: that one is drawn
    // beneath the selection widget and only shows when nothing covers it
    for (const QRect& stepped :
         { stepPast(above, true), stepPast(below, false) }) {
        if (!stepped.isNull()) {
            return stepped;
        }
    }
    if (fits(inside)) {
        return inside;
    }

    // Nothing fits. The caller draws no hint at all rather than covering
    // something the user needs.
    return {};
}

} // namespace HintPlacement
