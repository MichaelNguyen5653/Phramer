// SPDX-License-Identifier: GPL-3.0-or-later

#include "tools/accept/accepttool.h"

#include <QtTest>

/**
 * @brief Guards the ordering every export route depends on.
 *
 * CaptureWidget paints the selected object's outline straight into
 * m_context.screenshot, which is the same pixmap the capture is exported
 * from. REQ_CLEAR_SELECTION is what rebuilds that pixmap without the
 * outline, so a tool that finishes a capture without emitting it first
 * bakes the selection decoration into the saved image.
 */
class TestExportSignals : public QObject
{
    Q_OBJECT

private slots:
    void acceptClearsTheSelectionBeforeFinishingTheCapture();
};

void TestExportSignals::acceptClearsTheSelectionBeforeFinishingTheCapture()
{
    AcceptTool tool;
    QVector<CaptureTool::Request> seen;
    connect(&tool,
            &CaptureTool::requestAction,
            this,
            [&seen](CaptureTool::Request r) { seen.append(r); });

    CaptureContext context;
    tool.pressed(context);

    const int cleared = seen.indexOf(CaptureTool::REQ_CLEAR_SELECTION);
    const int done = seen.indexOf(CaptureTool::REQ_CAPTURE_DONE_OK);

    QVERIFY2(cleared != -1,
             "Accept must ask for the object selection to be cleared, or the "
             "outline is exported with the capture");
    QVERIFY2(done != -1, "Accept must mark the capture as done");
    QVERIFY2(cleared < done,
             "The selection has to be cleared before the capture is marked "
             "done, not after");
}

QTEST_MAIN(TestExportSignals)
#include "tst_exportsignals.moc"
