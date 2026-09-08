// SPDX-License-Identifier: GPL-3.0-or-later

#include "widgets/capture/capturetoolobjects.h"

#include <QPainter>
#include <QPixmap>
#include <QtTest>

namespace {

const QRect BoxArea(40, 40, 120, 60);
// Well inside the box but clear of the "glyphs", and far enough from them
// that the finder's search radius cannot reach one
const QPoint EmptyInterior(140, 90);

/**
 * @brief A tool that paints far less than it occupies, the way text does.
 *
 * TextTool paints glyphs, so most of the rectangle it reports as its
 * bounding box is transparent. This stub reproduces that shape without
 * dragging TextTool's ConfigHandler and QWidget dependencies into the test
 * binary.
 */
class SparseTool : public CaptureTool
{
public:
    explicit SparseTool(bool fillsSearchArea, QObject* parent = nullptr)
      : CaptureTool(parent)
      , m_fillsSearchArea(fillsSearchArea)
    {}

    bool isValid() const override { return true; }
    bool closeOnButtonPressed() const override { return false; }
    bool isSelectable() const override { return true; }
    bool showMousePreview() const override { return false; }
    QRect boundingRect() const override { return BoxArea; }

    QIcon icon(const QColor&, bool) const override { return {}; }
    QString name() const override { return QStringLiteral("sparse"); }
    CaptureTool::Type type() const override { return CaptureTool::TYPE_TEXT; }
    QString description() const override { return name(); }

    // Two small marks near the left edge stand in for glyphs
    void process(QPainter& painter, const QPixmap&) override
    {
        painter.fillRect(QRect(BoxArea.topLeft(), QSize(6, 6)), Qt::red);
        painter.fillRect(QRect(BoxArea.left(), BoxArea.top() + 20, 6, 6),
                         Qt::red);
    }

    void drawSearchArea(QPainter& painter, const QPixmap& pixmap) override
    {
        if (!m_fillsSearchArea) {
            CaptureTool::drawSearchArea(painter, pixmap);
            return;
        }
        painter.fillRect(BoxArea, Qt::red);
    }

    CaptureTool* copy(QObject* parent) override
    {
        return new SparseTool(m_fillsSearchArea, parent);
    }

    void paintMousePreview(QPainter&, const CaptureContext&) override {}
    void drawEnd(const QPoint&) override {}
    void drawMove(const QPoint&) override {}
    void drawStart(const CaptureContext&) override {}
    void pressed(CaptureContext&) override {}
    void onColorChanged(const QColor&) override {}
    void onSizeChanged(int) override {}

private:
    bool m_fillsSearchArea;
};

} // namespace

class TestToolPicking : public QObject
{
    Q_OBJECT

private slots:
    void glyphOnlyToolIsNotGrabbableInItsEmptyInterior();
    void fillingTheSearchAreaMakesTheWholeBoxGrabbable();
    void searchAreaDoesNotChangeWhatGetsPainted();
};

// Documents the behaviour that makes a text box hard to grab: picking tests
// the pixels a tool paints, so the gaps between glyphs are dead space.
void TestToolPicking::glyphOnlyToolIsNotGrabbableInItsEmptyInterior()
{
    CaptureToolObjects objects;
    QPointer<CaptureTool> tool = new SparseTool(false);
    objects.append(tool);

    QCOMPARE(objects.find(EmptyInterior, QSize(400, 300)), -1);
    // ...while a click on a painted mark still finds it
    QCOMPARE(objects.find(BoxArea.topLeft() + QPoint(2, 2), QSize(400, 300)),
             0);
    delete tool;
}

void TestToolPicking::fillingTheSearchAreaMakesTheWholeBoxGrabbable()
{
    CaptureToolObjects objects;
    QPointer<CaptureTool> tool = new SparseTool(true);
    objects.append(tool);

    // The same empty spot now hits, which is what lets the box be dragged
    QCOMPARE(objects.find(EmptyInterior, QSize(400, 300)), 0);
    QCOMPARE(objects.find(BoxArea.center(), QSize(400, 300)), 0);
    delete tool;
}

void TestToolPicking::searchAreaDoesNotChangeWhatGetsPainted()
{
    SparseTool tool(true);
    QPixmap canvas(400, 300);
    canvas.fill(Qt::transparent);
    QPainter painter(&canvas);
    tool.process(painter, canvas);
    painter.end();
    const QImage image = canvas.toImage();

    // Widening the grab area must not widen the annotation itself, or the
    // exported image would gain a filled block where the text is
    QVERIFY(qAlpha(image.pixel(BoxArea.topLeft() + QPoint(2, 2))) != 0);
    QCOMPARE(qAlpha(image.pixel(EmptyInterior)), 0);
}

QTEST_MAIN(TestToolPicking)
#include "tst_toolpicking.moc"
