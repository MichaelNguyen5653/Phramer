// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QPainter>

/**
 * @brief The compositing that gives highlighter output its look.
 *
 * Multiply blending at reduced opacity is what makes a highlight darken
 * whatever it covers instead of hiding it, and makes two overlapping strokes
 * darken twice. The opacity is applied to the painter rather than to the
 * colour's alpha channel: the two are not equivalent under multiply
 * blending, so changing that would change how overlaps look.
 *
 * Every tool that draws a highlight must go through this, so that the tools
 * stay consistent with each other and the value lives in one place.
 */
namespace HighlightStyle {

constexpr QPainter::CompositionMode CompositionMode =
  QPainter::CompositionMode_Multiply;
constexpr qreal Opacity = 0.35;

/**
 * @brief The border that keeps a highlight visible on dark captures.
 *
 * Multiply can only darken, so over a black background the fill resolves to
 * the background itself and the shape disappears. Closed highlight shapes
 * therefore stroke an outline at normal composition, outside the PainterState
 * scope so it never composites over its own fill. Freehand strokes
 * deliberately do not: a marker with a border stops reading as one.
 *
 * The width is the user's `highlightOutlineWidth` setting, snapshotted by
 * each tool when a drag starts so that placed objects keep the border they
 * were drawn with. Zero means no border, which is the pure-multiply look
 * this had before the setting existed.
 */
constexpr int MaxOutlineWidth = 20;

/**
 * @brief Applies the highlight compositing for the current scope.
 *
 * Restores the painter's previous composition mode and opacity on
 * destruction. Pen and brush are left to the caller, since what is being
 * drawn differs per tool.
 */
class PainterState
{
public:
    explicit PainterState(QPainter& painter)
      : m_painter(painter)
      , m_compositionMode(painter.compositionMode())
      , m_opacity(painter.opacity())
    {
        m_painter.setCompositionMode(CompositionMode);
        m_painter.setOpacity(Opacity);
    }

    ~PainterState()
    {
        m_painter.setOpacity(m_opacity);
        m_painter.setCompositionMode(m_compositionMode);
    }

    PainterState(const PainterState&) = delete;
    PainterState& operator=(const PainterState&) = delete;

private:
    QPainter& m_painter;
    QPainter::CompositionMode m_compositionMode;
    qreal m_opacity;
};

} // namespace HighlightStyle
