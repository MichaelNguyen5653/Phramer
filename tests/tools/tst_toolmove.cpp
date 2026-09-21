// SPDX-License-Identifier: GPL-3.0-or-later

#include "tools/abstractpathtool.h"
#include "tools/abstracttwopointtool.h"

#include <QPainter>
#include <QPixmap>
#include <QtTest>

namespace {

/**
 * @brief Concrete AbstractTwoPointTool that paints a solid rectangle.
 *
 * Every shape tool that can be moved derives from AbstractTwoPointTool, so
 * exercising the base is what proves the invariant for rectangle, circle,
 * arrow, line, marker, pixelate and invert alike.
 */
class TwoPointStub : public AbstractTwoPointTool
{
public:
    explicit TwoPointStub(QObject* parent = nullptr)
      : AbstractTwoPointTool(parent)
    {}

    QIcon icon(const QColor&, bool) const override { return {}; }
    QString name() const override { return QStringLiteral("stub"); }
    CaptureTool::Type type() const override
    {
        return CaptureTool::TYPE_RECTANGLE;
    }
    QString description() const override { return name(); }

    void process(QPainter& painter, const QPixmap&) override
    {
        painter.fillRect(QRect(points().first, points().second), Qt::red);
    }

    CaptureTool* copy(QObject* parent) override
    {
        auto* tool = new TwoPointStub(parent);
        copyParams(this, tool);
        return tool;
    }

    void pressed(CaptureContext&) override {}

    // Places the shape the way a completed drag would.
    void place(const QPoint& from, const QPoint& to)
    {
        CaptureContext context;
        context.mousePos = from;
        context.toolSize = 1;
        drawStart(context);
        drawMove(to);
        drawEnd(to);
    }
};

class PathStub : public AbstractPathTool
{
public:
    explicit PathStub(QObject* parent = nullptr)
      : AbstractPathTool(parent)
    {}

    QIcon icon(const QColor&, bool) const override { return {}; }
    QString name() const override { return QStringLiteral("stub"); }
    CaptureTool::Type type() const override { return CaptureTool::TYPE_PENCIL; }
    QString description() const override { return name(); }

    void process(QPainter& painter, const QPixmap&) override
    {
        painter.setPen(QPen(Qt::red, 3));
        for (const QPoint& point : m_points) {
            painter.drawPoint(point);
        }
    }

    CaptureTool* copy(QObject* parent) override
    {
        auto* tool = new PathStub(parent);
        copyParams(this, tool);
        return tool;
    }

    void pressed(CaptureContext&) override {}
    void drawStart(const CaptureContext& context) override
    {
        addPoint(context.mousePos);
    }
    void paintMousePreview(QPainter&, const CaptureContext&) override {}

    void place(const QVector<QPoint>& points)
    {
        for (const QPoint& point : points) {
            addPoint(point);
        }
    }
};

/**
 * @brief Renders a tool the way the export path does and returns the pixels.
 *
 * CaptureWidget composites placed objects by calling process() onto the
 * screenshot, so rendering here is the same operation that produces the
 * saved image.
 */
QImage render(CaptureTool& tool, const QSize& size = QSize(200, 200))
{
    QPixmap canvas(size);
    canvas.fill(Qt::transparent);
    QPainter painter(&canvas);
    tool.process(painter, canvas);
    painter.end();
    return canvas.toImage();
}

bool isPainted(const QImage& image, const QPoint& point)
{
    return qAlpha(image.pixel(point)) != 0;
}

} // namespace

class TestToolMove : public QObject
{
    Q_OBJECT

private slots:
    void twoPointMovePreservesShape();
    void twoPointRenderFollowsMove();
    void twoPointCopyCarriesMovedPosition();
    void pathMovePreservesShape();
    void pathRenderFollowsMove();
    void pathCopyCarriesMovedPosition();
    void twoPointTranslateShiftsBothPoints();
    void pathTranslateShiftsEveryPoint();
};

void TestToolMove::twoPointMovePreservesShape()
{
    TwoPointStub tool;
    tool.place(QPoint(10, 10), QPoint(50, 40));
    const QSize before = tool.boundingRect().size();

    tool.move(QPoint(100, 100));

    QCOMPARE(tool.points().first, QPoint(100, 100));
    QCOMPARE(tool.points().second, QPoint(140, 130));
    // A move must translate only; the drawn size may not drift
    QCOMPARE(tool.boundingRect().size(), before);
    QCOMPARE(*tool.pos(), QPoint(100, 100));
}

void TestToolMove::twoPointRenderFollowsMove()
{
    TwoPointStub tool;
    tool.place(QPoint(10, 10), QPoint(50, 40));
    QVERIFY(isPainted(render(tool), QPoint(30, 25)));

    tool.move(QPoint(100, 100));
    const QImage after = render(tool);

    // The shape renders at the new position and nowhere near the old one
    QVERIFY(isPainted(after, QPoint(120, 115)));
    QVERIFY(!isPainted(after, QPoint(30, 25)));
}

void TestToolMove::twoPointCopyCarriesMovedPosition()
{
    TwoPointStub tool;
    tool.place(QPoint(10, 10), QPoint(50, 40));
    tool.move(QPoint(100, 100));

    // Placed objects are stored as copies and re-copied on every undo
    // snapshot, so a move that copyParams drops would silently revert on
    // export.
    QScopedPointer<CaptureTool> duplicate(tool.copy(nullptr));

    QCOMPARE(duplicate->boundingRect(), tool.boundingRect());
    QVERIFY(isPainted(render(*duplicate), QPoint(120, 115)));
    QVERIFY(!isPainted(render(*duplicate), QPoint(30, 25)));
}

void TestToolMove::pathMovePreservesShape()
{
    PathStub tool;
    tool.place({ QPoint(10, 10), QPoint(20, 30), QPoint(40, 15) });
    const QSize before = tool.boundingRect().size();

    tool.move(QPoint(100, 100));

    QCOMPARE(*tool.pos(), QPoint(100, 100));
    QCOMPARE(tool.boundingRect().size(), before);
}

void TestToolMove::pathRenderFollowsMove()
{
    PathStub tool;
    tool.place({ QPoint(10, 10), QPoint(20, 30), QPoint(40, 15) });
    QVERIFY(isPainted(render(tool), QPoint(20, 30)));

    // pos() is the top-left of the path, so the first point lands at the
    // move target plus its own offset within the path
    tool.move(QPoint(100, 100));
    const QImage after = render(tool);

    QVERIFY(isPainted(after, QPoint(110, 120)));
    QVERIFY(!isPainted(after, QPoint(20, 30)));
}

void TestToolMove::pathCopyCarriesMovedPosition()
{
    PathStub tool;
    tool.place({ QPoint(10, 10), QPoint(20, 30), QPoint(40, 15) });
    tool.move(QPoint(100, 100));

    QScopedPointer<CaptureTool> duplicate(tool.copy(nullptr));

    QCOMPARE(duplicate->boundingRect(), tool.boundingRect());
    QVERIFY(isPainted(render(*duplicate), QPoint(110, 120)));
}

void TestToolMove::twoPointTranslateShiftsBothPoints()
{
    TwoPointStub tool;
    tool.place(QPoint(110, 90), QPoint(150, 120));

    // The overlay hands objects to the editor in selection-relative space,
    // which is a shift by minus the selection's corner
    tool.translate(QPoint(-100, -80));

    QCOMPARE(tool.points().first, QPoint(10, 10));
    QCOMPARE(tool.points().second, QPoint(50, 40));
}

void TestToolMove::pathTranslateShiftsEveryPoint()
{
    PathStub tool;
    tool.place({ QPoint(110, 90), QPoint(120, 110), QPoint(140, 95) });
    const QSize before = tool.boundingRect().size();

    tool.translate(QPoint(-100, -80));

    QCOMPARE(tool.boundingRect().size(), before);
    QVERIFY(isPainted(render(tool), QPoint(20, 30)));
    QVERIFY(!isPainted(render(tool), QPoint(120, 110)));
}

QTEST_MAIN(TestToolMove)
#include "tst_toolmove.moc"
