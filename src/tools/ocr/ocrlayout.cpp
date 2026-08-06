// SPDX-License-Identifier: GPL-3.0-or-later

#include "ocrlayout.h"

#include <algorithm>

namespace {

// Two boxes belong to the same visual row when they overlap vertically by
// more than this fraction of the shorter one
constexpr qreal RowOverlapFraction = 0.5;

// A gutter must be at least this many times the median glyph height, and
// this fraction of the total width, before it is accepted as a column break.
// Both are needed: the first rejects ordinary word spacing, the second
// rejects a ragged right margin in a narrow capture.
constexpr qreal MinGutterInGlyphs = 2.0;
constexpr qreal MinGutterInWidths = 0.04;

// Splitting needs enough lines to be evidence of a layout rather than an
// accident of two short lines
constexpr int MinLinesToSplit = 4;
constexpr int MinLinesPerColumn = 2;

// Wrapped-paragraph guards
constexpr qreal MaxHeightRatio = 1.5;
constexpr qreal MinRowAdvance = 0.5;
constexpr qreal MaxLineGap = 0.6;

qreal medianGlyphHeight(const QVector<OcrLine>& lines)
{
    QVector<qreal> heights;
    for (const OcrLine& line : lines) {
        if (line.boundingBox.height() > 0) {
            heights.append(line.boundingBox.height());
        }
    }
    if (heights.isEmpty()) {
        return 0.0;
    }
    std::sort(heights.begin(), heights.end());
    return heights[heights.size() / 2];
}

bool sharesRow(const QRectF& a, const QRectF& b)
{
    const qreal overlap = qMin(a.bottom(), b.bottom()) - qMax(a.top(), b.top());
    const qreal shorter = qMin(a.height(), b.height());
    if (shorter <= 0) {
        return false;
    }
    return overlap > shorter * RowOverlapFraction;
}

/**
 * Find the x positions of vertical gutters wide enough to be column breaks.
 * A gutter is a span of x that no line's bounding box crosses; a line that
 * spans the whole width (a heading above both columns) therefore suppresses
 * every gutter, which is the conservative answer.
 */
QVector<qreal> findGutters(const QVector<OcrLine>& lines)
{
    QVector<QPair<qreal, qreal>> spans;
    qreal left = 0.0;
    qreal right = 0.0;
    bool first = true;
    for (const OcrLine& line : lines) {
        const QRectF& box = line.boundingBox;
        if (box.width() <= 0) {
            continue;
        }
        spans.append({ box.left(), box.right() });
        left = first ? box.left() : qMin(left, box.left());
        right = first ? box.right() : qMax(right, box.right());
        first = false;
    }
    if (spans.size() < MinLinesToSplit || right <= left) {
        return {};
    }

    const qreal glyph = medianGlyphHeight(lines);
    const qreal minGutter =
      qMax(glyph * MinGutterInGlyphs, (right - left) * MinGutterInWidths);
    if (minGutter <= 0) {
        return {};
    }

    std::sort(spans.begin(), spans.end());

    QVector<qreal> gutters;
    qreal reach = spans.first().second;
    for (int i = 1; i < spans.size(); ++i) {
        if (spans[i].first - reach > minGutter) {
            // Split at the middle of the empty span
            gutters.append((reach + spans[i].first) / 2.0);
        }
        reach = qMax(reach, spans[i].second);
    }
    return gutters;
}

/// Order the lines of a single column band: visual rows top to bottom,
/// left to right within a row.
void appendOrderedBand(const QVector<OcrLine>& band, QVector<OcrLine>& out)
{
    QVector<OcrLine> sorted = band;
    std::stable_sort(
      sorted.begin(), sorted.end(), [](const OcrLine& a, const OcrLine& b) {
          return a.boundingBox.top() < b.boundingBox.top();
      });

    int i = 0;
    while (i < sorted.size()) {
        // Compare against the row's first line rather than a growing union,
        // which would let one tall line swallow the rest of the column
        int end = i + 1;
        while (end < sorted.size() &&
               sharesRow(sorted[i].boundingBox, sorted[end].boundingBox)) {
            ++end;
        }
        std::stable_sort(sorted.begin() + i,
                         sorted.begin() + end,
                         [](const OcrLine& a, const OcrLine& b) {
                             return a.boundingBox.left() < b.boundingBox.left();
                         });
        for (int k = i; k < end; ++k) {
            out.append(sorted[k]);
        }
        i = end;
    }
}

} // namespace

QVector<OcrLine> ocrOrderLines(const QVector<OcrLine>& lines)
{
    if (lines.size() < 2) {
        return lines;
    }

    QVector<QVector<OcrLine>> bands;
    const QVector<qreal> gutters = findGutters(lines);

    if (gutters.isEmpty()) {
        bands.append(lines);
    } else {
        bands.resize(gutters.size() + 1);
        for (const OcrLine& line : lines) {
            // The left edge decides the band, so a line that slightly
            // overhangs a gutter still stays with its own column
            int band = 0;
            while (band < gutters.size() &&
                   line.boundingBox.left() >= gutters[band]) {
                ++band;
            }
            bands[band].append(line);
        }

        // A band holding a single stray fragment is evidence the gutter was
        // an artefact, not a layout; fall back to treating the page as one
        const bool everyBandSubstantial = std::all_of(
          bands.cbegin(), bands.cend(), [](const QVector<OcrLine>& band) {
              return band.size() >= MinLinesPerColumn;
          });
        if (!everyBandSubstantial) {
            bands.clear();
            bands.append(lines);
        }
    }

    QVector<OcrLine> ordered;
    ordered.reserve(lines.size());
    for (const QVector<OcrLine>& band : bands) {
        appendOrderedBand(band, ordered);
    }
    return ordered;
}

bool ocrIsWrappedContinuation(const QRectF& current, const QRectF& next)
{
    const qreal currentHeight = current.height();
    const qreal nextHeight = next.height();
    if (currentHeight <= 0 || nextHeight <= 0) {
        return false;
    }

    // Lines set in noticeably different sizes belong to different blocks
    const qreal minHeight = qMin(currentHeight, nextHeight);
    const qreal maxHeight = qMax(currentHeight, nextHeight);
    if (maxHeight / minHeight > MaxHeightRatio) {
        return false;
    }

    // Consecutive entries must advance to a distinct visual row. Without
    // this, two fragments of the same row have a negative gap and would be
    // merged into a one-line-tall "paragraph".
    if (next.top() - current.top() < minHeight * MinRowAdvance) {
        return false;
    }

    // Normal leading means the same paragraph; extra whitespace is a break
    const qreal gap = next.top() - current.bottom();
    const qreal averageHeight = (currentHeight + nextHeight) / 2.0;
    return gap < averageHeight * MaxLineGap;
}

qreal ocrEstimateCharWidth(const QVector<OcrLine>& lines)
{
    QVector<qreal> widths;
    for (const OcrLine& line : lines) {
        if (line.words.isEmpty()) {
            const int length = line.text.trimmed().length();
            if (length > 0 && line.boundingBox.width() > 0) {
                widths.append(line.boundingBox.width() / length);
            }
            continue;
        }
        for (const OcrWord& word : line.words) {
            const int length = word.text.trimmed().length();
            if (length > 0 && word.boundingBox.width() > 0) {
                widths.append(word.boundingBox.width() / length);
            }
        }
    }
    if (widths.isEmpty()) {
        return 0.0;
    }
    // The median survives an oversized heading or a one-character fragment
    // that a mean would let distort every indent on the page
    std::sort(widths.begin(), widths.end());
    return widths[widths.size() / 2];
}

namespace {

struct Row
{
    QString text;
    QRectF box;
};

/// Turn a horizontal distance into the number of spaces that spans it
int spacesFor(qreal distance, qreal charWidth)
{
    if (charWidth <= 0.0 || distance <= 0.0) {
        return 0;
    }
    return int(qRound(distance / charWidth));
}

/**
 * Collapse the ordered fragments into one entry per visual row, putting the
 * whitespace back. Fragments that share a row arrive adjacent because
 * ocrOrderLines() already grouped them.
 */
QVector<Row> buildRows(const QVector<OcrLine>& lines,
                       OcrTextLayout layout,
                       qreal charWidth)
{
    const bool preserveSpacing = layout != OcrTextLayout::Plain;

    qreal leftMargin = 0.0;
    bool haveMargin = false;
    for (const OcrLine& line : lines) {
        if (line.boundingBox.width() <= 0) {
            continue;
        }
        leftMargin = haveMargin ? qMin(leftMargin, line.boundingBox.left())
                                : line.boundingBox.left();
        haveMargin = true;
    }

    QVector<Row> rows;
    int index = 0;
    while (index < lines.size()) {
        int end = index + 1;
        while (end < lines.size() &&
               sharesRow(lines[index].boundingBox, lines[end].boundingBox)) {
            ++end;
        }

        Row row;
        qreal previousRight = 0.0;
        bool first = true;
        for (int k = index; k < end; ++k) {
            const OcrLine& fragment = lines[k];
            const QString text = fragment.text.trimmed();
            if (text.isEmpty()) {
                continue;
            }
            if (first) {
                if (preserveSpacing && haveMargin) {
                    // Indentation is the distance from the leftmost thing on
                    // the page, which is what makes nested code line up
                    row.text += QString(
                      spacesFor(fragment.boundingBox.left() - leftMargin,
                                charWidth),
                      QLatin1Char(' '));
                }
            } else if (preserveSpacing) {
                // A single space would collapse column alignment, so the gap
                // decides; never fewer than one or words would run together
                row.text += QString(
                  qMax(1,
                       spacesFor(fragment.boundingBox.left() - previousRight,
                                 charWidth)),
                  QLatin1Char(' '));
            } else {
                row.text += QLatin1Char(' ');
            }
            row.text += text;
            row.box = row.box.isNull() ? fragment.boundingBox
                                       : row.box.united(fragment.boundingBox);
            previousRight = fragment.boundingBox.right();
            first = false;
        }

        if (!row.text.trimmed().isEmpty()) {
            rows.append(row);
        }
        index = end;
    }
    return rows;
}

} // namespace

QString ocrAssembleText(const QVector<OcrLine>& lines, OcrTextLayout layout)
{
    const qreal charWidth = ocrEstimateCharWidth(lines);
    const QVector<Row> rows = buildRows(lines, layout, charWidth);

    QString text;
    for (int i = 0; i < rows.size(); ++i) {
        if (i > 0) {
            const bool continuation =
              layout == OcrTextLayout::Join &&
              ocrIsWrappedContinuation(rows[i - 1].box, rows[i].box);
            text += continuation ? QLatin1Char(' ') : QLatin1Char('\n');
            if (continuation) {
                // A joined line is one paragraph, so the second row's own
                // indentation would appear as a gap in the middle of it
                text += rows[i].text.trimmed();
                continue;
            }
        }
        text += rows[i].text;
    }
    // Trailing indent on the last row is invisible but survives a paste
    return text;
}
