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
    const QVector<QRect> candidates{
        centredAt(selection.top() - gap - hint.height()),
        centredAt(selection.bottom() + 1 + gap),
        centredAt(selection.bottom() + 1 - gap - hint.height()),
    };

    for (const QRect& candidate : candidates) {
        if (!screen.contains(candidate)) {
            continue;
        }
        bool blocked = false;
        for (const QRect& obstacle : obstacles) {
            if (candidate.intersects(obstacle)) {
                blocked = true;
                break;
            }
        }
        if (!blocked) {
            return candidate;
        }
    }

    // Nothing fits. The caller draws no hint at all rather than covering
    // something the user needs.
    return {};
}

} // namespace HintPlacement
