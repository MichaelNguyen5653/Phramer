// SPDX-License-Identifier: GPL-3.0-or-later

#include "frostedfill.h"

#include <algorithm>
#include <array>
#include <random>
#include <vector>

namespace {

struct Rgb
{
    float r = 0;
    float g = 0;
    float b = 0;
};

Rgb operator+(Rgb a, Rgb b)
{
    return { a.r + b.r, a.g + b.g, a.b + b.b };
}
Rgb operator-(Rgb a, Rgb b)
{
    return { a.r - b.r, a.g - b.g, a.b - b.b };
}
Rgb operator*(Rgb a, float s)
{
    return { a.r * s, a.g * s, a.b * s };
}

using Line = std::vector<Rgb>;

QVector<QRgb> readLine(const QPixmap& source, const QRect& strip)
{
    const QImage image = source.copy(strip).toImage();
    QVector<QRgb> line;
    line.reserve(strip.width() * strip.height());
    for (int y = 0; y < image.height(); ++y) {
        for (int x = 0; x < image.width(); ++x) {
            line.append(image.pixel(x, y));
        }
    }
    return line;
}

Line toLine(const QVector<QRgb>& pixels)
{
    Line line;
    line.reserve(pixels.size());
    for (QRgb pixel : pixels) {
        line.push_back(
          { qRed(pixel) / 255.f, qGreen(pixel) / 255.f, qBlue(pixel) / 255.f });
    }
    return line;
}

// Three box passes approximate a Gaussian in linear time, which matters
// because a side can be thousands of pixels long and this runs while the
// rectangle is being dragged
void smooth(Line& line, int radius)
{
    const int n = static_cast<int>(line.size());
    if (n < 2 || radius < 1) {
        return;
    }
    Line prefix(n + 1);
    for (int pass = 0; pass < 3; ++pass) {
        for (int i = 0; i < n; ++i) {
            prefix[i + 1] = prefix[i] + line[i];
        }
        for (int i = 0; i < n; ++i) {
            const int lo = std::max(0, i - radius);
            const int hi = std::min(n - 1, i + radius);
            line[i] = (prefix[hi + 1] - prefix[lo]) * (1.f / (hi - lo + 1));
        }
    }
}

Rgb sample(const Line& line, float t)
{
    if (line.size() == 1) {
        return line.front();
    }
    const float position = t * static_cast<float>(line.size() - 1);
    const int index =
      std::clamp(static_cast<int>(position), 0, int(line.size()) - 2);
    const float fraction = position - static_cast<float>(index);
    return line[index] * (1 - fraction) + line[index + 1] * fraction;
}

Rgb average(std::initializer_list<const Rgb*> candidates, Rgb fallback)
{
    Rgb sum;
    int count = 0;
    for (const Rgb* candidate : candidates) {
        if (candidate) {
            sum = sum + *candidate;
            ++count;
        }
    }
    return count ? sum * (1.f / count) : fallback;
}

Line gradient(Rgb from, Rgb to, int length)
{
    Line line(std::max(length, 1));
    for (int i = 0; i < int(line.size()); ++i) {
        const float t =
          line.size() > 1 ? float(i) / float(line.size() - 1) : 0.f;
        line[i] = from * (1 - t) + to * t;
    }
    return line;
}

// A fixed tile rather than per-pixel randomness: the fill is recomputed
// while dragging, and a tile keeps the grain identical from frame to frame
// instead of shimmering
const std::array<int, 64 * 64>& grainTile()
{
    static const std::array<int, 64 * 64> tile = [] {
        std::array<int, 64 * 64> values{};
        std::mt19937 prng(0x5eed);
        std::uniform_int_distribution<int> amplitude(-3, 3);
        for (int& value : values) {
            value = amplitude(prng);
        }
        return values;
    }();
    return tile;
}

} // namespace

FrostedEdges frostedEdges(const QPixmap& source, const QRect& rect)
{
    FrostedEdges edges;
    const QRect bounds = source.rect();
    const QRect r = rect.intersected(bounds);
    if (r.isEmpty()) {
        return edges;
    }
    if (r.top() > bounds.top()) {
        edges.top =
          readLine(source, QRect(r.left(), r.top() - 1, r.width(), 1));
    }
    if (r.bottom() < bounds.bottom()) {
        edges.bottom =
          readLine(source, QRect(r.left(), r.bottom() + 1, r.width(), 1));
    }
    if (r.left() > bounds.left()) {
        edges.left =
          readLine(source, QRect(r.left() - 1, r.top(), 1, r.height()));
    }
    if (r.right() < bounds.right()) {
        edges.right =
          readLine(source, QRect(r.right() + 1, r.top(), 1, r.height()));
    }
    return edges;
}

QImage frostedFill(const FrostedEdges& edges, const QSize& size, int strength)
{
    if (size.isEmpty()) {
        return {};
    }
    const int width = size.width();
    const int height = size.height();

    Line top = toLine(edges.top);
    Line bottom = toLine(edges.bottom);
    Line left = toLine(edges.left);
    Line right = toLine(edges.right);

    // Softer with a larger tool size, but never so little that the edge
    // detail of what surrounds the box survives into it
    const float spread = std::clamp(0.03f + strength * 0.006f, 0.03f, 0.3f);
    smooth(top, static_cast<int>(width * spread));
    smooth(bottom, static_cast<int>(width * spread));
    smooth(left, static_cast<int>(height * spread));
    smooth(right, static_cast<int>(height * spread));

    // A side with nothing outside it is bridged between its neighbours'
    // ends, so a box against the image edge still reads nothing inside
    auto front = [](const Line& l) { return l.empty() ? nullptr : &l.front(); };
    auto back = [](const Line& l) { return l.empty() ? nullptr : &l.back(); };
    Line all;
    for (const Line* line : { &top, &bottom, &left, &right }) {
        all.insert(all.end(), line->begin(), line->end());
    }
    Rgb mean{ 0.5f, 0.5f, 0.5f };
    if (!all.empty()) {
        Rgb sum;
        for (const Rgb& c : all) {
            sum = sum + c;
        }
        mean = sum * (1.f / all.size());
    }
    const Rgb topLeft = average({ front(top), front(left) }, mean);
    const Rgb topRight = average({ back(top), front(right) }, mean);
    const Rgb bottomLeft = average({ front(bottom), back(left) }, mean);
    const Rgb bottomRight = average({ back(bottom), back(right) }, mean);
    if (top.empty()) {
        top = gradient(topLeft, topRight, width);
    }
    if (bottom.empty()) {
        bottom = gradient(bottomLeft, bottomRight, width);
    }
    if (left.empty()) {
        left = gradient(topLeft, bottomLeft, height);
    }
    if (right.empty()) {
        right = gradient(topRight, bottomRight, height);
    }

    // The patch is smooth, so a coarse grid upscaled with filtering looks the
    // same as evaluating every pixel and costs a fraction of it
    const int gridWidth = std::clamp(width / 6, 2, 400);
    const int gridHeight = std::clamp(height / 6, 2, 400);
    QImage grid(gridWidth, gridHeight, QImage::Format_RGB32);
    for (int gy = 0; gy < gridHeight; ++gy) {
        const float v = float(gy) / float(gridHeight - 1);
        auto* row = reinterpret_cast<QRgb*>(grid.scanLine(gy));
        for (int gx = 0; gx < gridWidth; ++gx) {
            const float u = float(gx) / float(gridWidth - 1);
            // Coons patch: the two ruled surfaces minus the bilinear corner
            // blend they both contain. It lands on every side exactly where
            // adjacent sides agree at their shared corner, and splits the
            // difference near a corner where they do not
            const Rgb c =
              sample(top, u) * (1 - v) + sample(bottom, u) * v +
              sample(left, v) * (1 - u) + sample(right, v) * u -
              (topLeft * ((1 - u) * (1 - v)) + topRight * (u * (1 - v)) +
               bottomLeft * ((1 - u) * v) + bottomRight * (u * v));
            row[gx] = qRgb(std::clamp(int(c.r * 255.f + 0.5f), 0, 255),
                           std::clamp(int(c.g * 255.f + 0.5f), 0, 255),
                           std::clamp(int(c.b * 255.f + 0.5f), 0, 255));
        }
    }

    QImage fill = grid.scaled(
      width, height, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
    fill = fill.convertToFormat(QImage::Format_RGB32);

    const auto& grain = grainTile();
    for (int y = 0; y < height; ++y) {
        auto* row = reinterpret_cast<QRgb*>(fill.scanLine(y));
        const int* grainRow = grain.data() + (y % 64) * 64;
        for (int x = 0; x < width; ++x) {
            const int n = grainRow[x % 64];
            const QRgb p = row[x];
            row[x] = qRgb(std::clamp(qRed(p) + n, 0, 255),
                          std::clamp(qGreen(p) + n, 0, 255),
                          std::clamp(qBlue(p) + n, 0, 255));
        }
    }
    return fill;
}
