// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QRect>
#include <QSize>
#include <QVector>

/**
 * @brief Where the capture overlay's keyboard hint can sit.
 *
 * Kept out of CaptureWidget so the candidate order and the rejection rules can
 * be tested without a fullscreen overlay. The hint is painted, not a widget,
 * so this returns a rectangle rather than moving anything.
 */
namespace HintPlacement {

// Distance between the hint and the selection edge.
inline constexpr int Gap = 8;

// Tries above the selection, then below. Above comes first because the tool
// buttons default to below. A candidate that leaves the screen or touches an
// obstacle is rejected. When both are blocked they are stepped outward past
// the obstacles in their way, and only then is the slot inside the
// selection's bottom edge tried. A null rectangle means nothing fits and the
// hint should not be drawn at all.
QRect place(const QSize& hint,
            const QRect& selection,
            const QRect& screen,
            const QVector<QRect>& obstacles,
            int gap = Gap);

} // namespace HintPlacement
