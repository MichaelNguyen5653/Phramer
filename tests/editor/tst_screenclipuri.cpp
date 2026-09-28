// SPDX-License-Identifier: GPL-3.0-or-later

#include "utils/screenclipprotocol.h"

#include <QtTest>

// Which ms-screenclip links Phramer hands back to Snipping Tool for recording
// instead of opening its own capture
class TestScreenClipUri : public QObject
{
    Q_OBJECT

private slots:
    void recordingLinksGoToSnippingTool()
    {
        QVERIFY(ScreenClipProtocol::isRecordingRequest(QStringLiteral(
          "ms-screenclip:capture?mode=default&type=recording&source=HotKey")));
        QVERIFY(ScreenClipProtocol::isRecordingRequest(
          QStringLiteral("ms-screenclip:?type=RECORDING")));
        QVERIFY(ScreenClipProtocol::isRecordingRequest(QStringLiteral(
          "ms-screenclip://capture/video?user-agent=App&redirect-uri=x://y")));
    }

    void snipLinksStayWithPhramer()
    {
        QVERIFY(!ScreenClipProtocol::isRecordingRequest(
          QStringLiteral("ms-screenclip:///?source=HotKey")));
        QVERIFY(!ScreenClipProtocol::isRecordingRequest(
          QStringLiteral("ms-screenclip:capture?mode=default&type=snapshot")));
        QVERIFY(!ScreenClipProtocol::isRecordingRequest(QStringLiteral(
          "ms-screenclip://capture/image?rectangle&redirect-uri=x://y")));
    }

    void noLinkIsACapture()
    {
        QVERIFY(!ScreenClipProtocol::isRecordingRequest(QString()));
    }
};

QTEST_APPLESS_MAIN(TestScreenClipUri)
#include "tst_screenclipuri.moc"
