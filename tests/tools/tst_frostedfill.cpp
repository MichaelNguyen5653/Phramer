// SPDX-License-Identifier: GPL-3.0-or-later

#include "tools/pixelate/frostedfill.h"

#include <QPainter>
#include <QtTest>

namespace {

// A busy backdrop, so a fill that leaked any interior pixel would differ
QPixmap backdrop(const QSize& size)
{
    QImage image(size, QImage::Format_RGB32);
    for (int y = 0; y < size.height(); ++y) {
        for (int x = 0; x < size.width(); ++x) {
            image.setPixel(x, y, qRgb((x * 7) % 256, (y * 5) % 256, 90));
        }
    }
    return QPixmap::fromImage(image);
}

// Stands in for text under the box: the thing that must not survive
QPixmap withSecret(QPixmap pixmap, const QRect& rect)
{
    QPainter painter(&pixmap);
    painter.fillRect(rect, Qt::white);
    painter.setPen(Qt::black);
    for (int y = rect.top(); y <= rect.bottom(); y += 3) {
        painter.drawLine(rect.left(), y, rect.right(), y);
    }
    painter.end();
    return pixmap;
}

QImage fillFor(const QPixmap& pixmap, const QRect& rect, int strength = 10)
{
    return frostedFill(frostedEdges(pixmap, rect), rect.size(), strength);
}

int channelDistance(QRgb a, QRgb b)
{
    return qMax(qAbs(qRed(a) - qRed(b)),
                qMax(qAbs(qGreen(a) - qGreen(b)), qAbs(qBlue(a) - qBlue(b))));
}

} // namespace

class TestFrostedFill : public QObject
{
    Q_OBJECT

private slots:
    void interiorIsNeverRead();
    void boxAgainstImageEdgeReadsNothingInside();
    void fillCoversRequestedSize();
    void uniformSurroundingsGiveUniformFill();
    void fillMeetsItsSurroundings();
    void sameInputGivesSameFill();
};

void TestFrostedFill::interiorIsNeverRead()
{
    const QRect box(40, 30, 120, 60);
    const QPixmap clean = backdrop(QSize(200, 140));
    const QPixmap secret = withSecret(clean, box);

    // The security property: changing only what the box covers changes
    // nothing about what is drawn over it
    QCOMPARE(fillFor(secret, box), fillFor(clean, box));
}

void TestFrostedFill::boxAgainstImageEdgeReadsNothingInside()
{
    // Flush with the top-left corner, where the only row "above" would be
    // the box's own first row
    const QRect box(0, 0, 80, 50);
    const QPixmap clean = backdrop(QSize(200, 140));
    const QPixmap secret = withSecret(clean, box);

    const FrostedEdges edges = frostedEdges(clean, box);
    QVERIFY(edges.top.isEmpty());
    QVERIFY(edges.left.isEmpty());
    QCOMPARE(fillFor(secret, box), fillFor(clean, box));

    // Covering the whole image leaves nothing to read at all
    const QRect everything(QPoint(0, 0), clean.size());
    QCOMPARE(fillFor(withSecret(clean, everything), everything),
             fillFor(clean, everything));
}

void TestFrostedFill::fillCoversRequestedSize()
{
    const QRect box(10, 10, 37, 23);
    const QImage fill = fillFor(backdrop(QSize(100, 100)), box);
    QCOMPARE(fill.size(), box.size());
}

void TestFrostedFill::uniformSurroundingsGiveUniformFill()
{
    QPixmap pixmap(120, 90);
    pixmap.fill(QColor(200, 100, 50));
    const QRect box(20, 20, 70, 40);
    const QImage fill = fillFor(withSecret(pixmap, box), box);

    // Everything is the surrounding colour give or take the grain
    const QRgb expected = qRgb(200, 100, 50);
    for (int y = 0; y < fill.height(); ++y) {
        for (int x = 0; x < fill.width(); ++x) {
            QVERIFY2(channelDistance(fill.pixel(x, y), expected) <= 4,
                     qPrintable(QStringLiteral("pixel %1,%2").arg(x).arg(y)));
        }
    }
}

void TestFrostedFill::fillMeetsItsSurroundings()
{
    // Red on the left fading to blue on the right, constant down each
    // column, so each side's colour is known exactly
    QImage image(200, 100, QImage::Format_RGB32);
    for (int x = 0; x < image.width(); ++x) {
        const int t = x * 255 / (image.width() - 1);
        for (int y = 0; y < image.height(); ++y) {
            image.setPixel(x, y, qRgb(255 - t, 0, t));
        }
    }
    const QPixmap pixmap = QPixmap::fromImage(image);
    const QRect box(50, 20, 100, 60);
    const QImage fill = fillFor(pixmap, box);

    // No seam: the fill's outer columns match the columns beside the box.
    // Not exactly: where two sides meet, their smoothed colours disagree and
    // the patch splits the difference, which reaches mid-side on a gradient
    // this steep (about 1.3 levels per pixel). The grain adds up to 3 more.
    const int middle = fill.height() / 2;
    const QRgb leftInside = fill.pixel(0, middle);
    const QRgb leftOutside = image.pixel(box.left() - 1, middle);
    const QRgb rightInside = fill.pixel(fill.width() - 1, middle);
    const QRgb rightOutside = image.pixel(box.right() + 1, middle);
    auto describe = [](QRgb inside, QRgb outside) {
        return QStringLiteral("inside %1 outside %2")
          .arg(QColor(inside).name(), QColor(outside).name());
    };
    QVERIFY2(channelDistance(leftInside, leftOutside) <= 12,
             qPrintable(describe(leftInside, leftOutside)));
    QVERIFY2(channelDistance(rightInside, rightOutside) <= 12,
             qPrintable(describe(rightInside, rightOutside)));
}

void TestFrostedFill::sameInputGivesSameFill()
{
    // A fill that changed between repaints would shimmer while dragging
    const QPixmap pixmap = backdrop(QSize(160, 120));
    const QRect box(30, 30, 90, 50);
    QCOMPARE(fillFor(pixmap, box), fillFor(pixmap, box));
}

QTEST_MAIN(TestFrostedFill)
#include "tst_frostedfill.moc"
