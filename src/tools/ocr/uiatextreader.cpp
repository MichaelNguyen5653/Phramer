// SPDX-License-Identifier: GPL-3.0-or-later

#include "uiatextreader.h"

#ifndef NOMINMAX
#define NOMINMAX
#endif

// Deliberately without WIN32_LEAN_AND_MEAN: that suppresses objbase.h, and
// with it the `interface` macro every UI Automation declaration is written
// in, which fails as a wall of syntax errors inside the SDK headers
#include <windows.h>

#include <objbase.h>
#include <oleauto.h>

#include <UIAutomation.h>
#include <dwmapi.h>
#include <wrl/client.h>

using Microsoft::WRL::ComPtr;

namespace {

// A provider that misbehaves must not hang the worker forever. These are
// generous enough that a busy but healthy application still answers.
constexpr int ConnectionTimeoutMs = 1000;
constexpr int TransactionTimeoutMs = 3000;

// Upper bound on the line walk. The cursor starts at the top of the
// selection, so this is only ever reached if a provider reports lines that
// do not advance.
constexpr int MaxLines = 500;

// How many overlapping windows to try before giving up. Windows without a
// text provider are rejected in a call or two, so this is cheap; the bound
// exists so a desktop full of stacked windows cannot turn into a long
// series of cross-process round trips.
constexpr int MaxCandidates = 6;

/**
 * Initializes a COM apartment for the calling thread and releases it again
 * on scope exit. Initialization fails harmlessly when the thread already
 * has an apartment of another type, in which case the existing one is left
 * alone. Declare before any ComPtr so those are released first.
 */
struct ApartmentScope
{
    bool owned = false;

    ApartmentScope()
    {
        owned = SUCCEEDED(CoInitializeEx(nullptr, COINIT_MULTITHREADED));
    }

    ~ApartmentScope()
    {
        if (owned) {
            CoUninitialize();
        }
    }
};

QString toQString(BSTR value)
{
    if (value == nullptr) {
        return {};
    }
    return QString::fromWCharArray(value,
                                   static_cast<int>(SysStringLen(value)));
}

/// Owning wrapper for the BSTRs the UI Automation API hands back
struct ScopedBstr
{
    BSTR value = nullptr;
    ~ScopedBstr()
    {
        if (value != nullptr) {
            SysFreeString(value);
        }
    }
    ScopedBstr() = default;
    ScopedBstr(const ScopedBstr&) = delete;
    ScopedBstr& operator=(const ScopedBstr&) = delete;
};

bool isCloaked(HWND window)
{
    // Windows keeps cloaked windows around for suspended UWP apps and for
    // tabs on other virtual desktops. They are invisible but still pass
    // IsWindowVisible, so hit-testing walks straight into them.
    BOOL cloaked = FALSE;
    if (SUCCEEDED(DwmGetWindowAttribute(
          window, DWMWA_CLOAKED, &cloaked, sizeof(cloaked)))) {
        return cloaked != FALSE;
    }
    return false;
}

/**
 * Windows whose rectangle covers `point`, topmost first, excluding our own.
 *
 * This deliberately returns candidates rather than an answer. A window
 * rectangle covering a point does not mean that window is what is drawn
 * there: overlapping windows are the normal case, and the desktop, a
 * maximized browser and a full-screen editor all cover most points on the
 * screen. Z-order does not settle it either, because it keeps changing as
 * the user works and the screenshot was frozen earlier. Whether a candidate
 * is the right one is decided by readFromWindow(), which checks that the
 * text it gets back is actually positioned inside the selection.
 *
 * WindowFromPoint cannot be used at all: the capture overlay covers the
 * screen and wins every hit test.
 */
QVector<HWND> candidateWindowsAt(const POINT& point, int limit)
{
    const DWORD ownProcess = GetCurrentProcessId();
    QVector<HWND> candidates;

    for (HWND window = GetTopWindow(nullptr);
         window != nullptr && candidates.size() < limit;
         window = GetNextWindow(window, GW_HWNDNEXT)) {
        DWORD process = 0;
        GetWindowThreadProcessId(window, &process);
        if (process == ownProcess) {
            continue;
        }
        if (IsWindowVisible(window) == FALSE || IsIconic(window) != FALSE) {
            continue;
        }
        RECT bounds;
        if (GetWindowRect(window, &bounds) == FALSE ||
            PtInRect(&bounds, point) == FALSE) {
            continue;
        }
        if (isCloaked(window)) {
            continue;
        }
        candidates.append(window);
    }
    return candidates;
}

ComPtr<IUIAutomation> createAutomation()
{
    // CUIAutomation8 is the same API plus the timeout knobs, so prefer it
    // and only fall back for the sake of older systems
    ComPtr<IUIAutomation2> automation2;
    if (SUCCEEDED(CoCreateInstance(CLSID_CUIAutomation8,
                                   nullptr,
                                   CLSCTX_INPROC_SERVER,
                                   IID_PPV_ARGS(&automation2))) &&
        automation2) {
        automation2->put_ConnectionTimeout(ConnectionTimeoutMs);
        automation2->put_TransactionTimeout(TransactionTimeoutMs);
        // Reading text must never steal focus from whatever the user is
        // actually doing
        automation2->put_AutoSetFocus(FALSE);
        return automation2;
    }

    ComPtr<IUIAutomation> automation;
    CoCreateInstance(CLSID_CUIAutomation,
                     nullptr,
                     CLSCTX_INPROC_SERVER,
                     IID_PPV_ARGS(&automation));
    return automation;
}

/// The element itself if it exposes text, otherwise the first descendant
/// that does. A window's text usually lives in a child control.
ComPtr<IUIAutomationElement> findTextElement(IUIAutomation* automation,
                                             IUIAutomationElement* root)
{
    ComPtr<IUIAutomationTextPattern> pattern;
    if (SUCCEEDED(root->GetCurrentPatternAs(UIA_TextPatternId,
                                            IID_PPV_ARGS(&pattern))) &&
        pattern) {
        return root;
    }

    VARIANT wanted;
    VariantInit(&wanted);
    wanted.vt = VT_BOOL;
    wanted.boolVal = VARIANT_TRUE;

    ComPtr<IUIAutomationCondition> condition;
    if (FAILED(automation->CreatePropertyCondition(
          UIA_IsTextPatternAvailablePropertyId, wanted, &condition))) {
        VariantClear(&wanted);
        return nullptr;
    }

    ComPtr<IUIAutomationElement> found;
    root->FindFirst(TreeScope_Descendants, condition.Get(), &found);
    VariantClear(&wanted);
    return found;
}

/// Union of the rectangles a text range occupies, in physical screen pixels
QRectF rangeBounds(IUIAutomationTextRange* range)
{
    SAFEARRAY* array = nullptr;
    if (FAILED(range->GetBoundingRectangles(&array)) || array == nullptr) {
        return {};
    }

    QRectF bounds;
    double* values = nullptr;
    if (SUCCEEDED(
          SafeArrayAccessData(array, reinterpret_cast<void**>(&values)))) {
        LONG lower = 0;
        LONG upper = -1;
        SafeArrayGetLBound(array, 1, &lower);
        SafeArrayGetUBound(array, 1, &upper);
        // Four doubles per rectangle: left, top, width, height
        const LONG count = upper - lower + 1;
        for (LONG i = 0; i + 3 < count; i += 4) {
            const QRectF rect(
              values[i], values[i + 1], values[i + 2], values[i + 3]);
            if (rect.width() > 0 && rect.height() > 0) {
                bounds = bounds.isNull() ? rect : bounds.united(rect);
            }
        }
        SafeArrayUnaccessData(array);
    }
    SafeArrayDestroy(array);
    return bounds;
}

/**
 * Try to read the selection's text out of one candidate window.
 *
 * Returns false unless the window yields text whose bounding boxes actually
 * lie inside `screenRect`. That check is what makes guessing at candidates
 * safe: a window that merely overlaps the selection reports its text
 * somewhere else entirely, and silently showing it as "exact" would be worse
 * than admitting there is none.
 */
bool readFromWindow(IUIAutomation* automation,
                    HWND window,
                    const QRect& screenRect,
                    OcrResult& out)
{
    ComPtr<IUIAutomationElement> root;
    if (FAILED(automation->ElementFromHandle(window, &root)) || !root) {
        return false;
    }

    ComPtr<IUIAutomationElement> textElement =
      findTextElement(automation, root.Get());
    if (!textElement) {
        return false;
    }

    ComPtr<IUIAutomationTextPattern> pattern;
    if (FAILED(textElement->GetCurrentPatternAs(UIA_TextPatternId,
                                                IID_PPV_ARGS(&pattern))) ||
        !pattern) {
        return false;
    }

    // Starting from the top of the selection keeps the walk short even when
    // the provider holds a large scrollback behind it
    const POINT start{ screenRect.center().x(), screenRect.top() + 1 };
    ComPtr<IUIAutomationTextRange> cursor;
    if (FAILED(pattern->RangeFromPoint(start, &cursor)) || !cursor) {
        if (FAILED(pattern->get_DocumentRange(&cursor)) || !cursor) {
            return false;
        }
    }
    cursor->ExpandToEnclosingUnit(TextUnit_Line);

    const QRectF wanted(screenRect);
    QVector<OcrLine> lines;
    QStringList texts;

    for (int i = 0; i < MaxLines; ++i) {
        const QRectF bounds = rangeBounds(cursor.Get());

        // Providers expose the scrollback above and below the viewport too,
        // so the walk has to stop itself rather than run to the end
        if (!bounds.isNull() && bounds.top() > wanted.bottom()) {
            break;
        }

        if (bounds.intersects(wanted)) {
            ScopedBstr text;
            if (SUCCEEDED(cursor->GetText(-1, &text.value))) {
                const QString line = toQString(text.value)
                                       .remove(QLatin1Char('\r'))
                                       .remove(QLatin1Char('\n'));
                if (!line.trimmed().isEmpty()) {
                    OcrLine ocrLine;
                    ocrLine.text = line;
                    ocrLine.boundingBox = bounds;
                    lines.append(ocrLine);
                    texts.append(line);
                }
            }
        }

        int moved = 0;
        if (FAILED(cursor->Move(TextUnit_Line, 1, &moved)) || moved == 0) {
            break;
        }
    }

    if (texts.isEmpty()) {
        return false;
    }

    out.lines = lines;
    out.fullText = texts.join(QLatin1Char('\n'));
    out.status = OcrResult::Status::Ok;
    return true;
}

} // namespace

OcrResult readWindowText(const QRect& screenRect)
{
    OcrResult result;
    result.status = OcrResult::Status::NoTextFound;

    if (screenRect.isEmpty()) {
        return result;
    }

    ApartmentScope apartment;

    ComPtr<IUIAutomation> automation = createAutomation();
    if (!automation) {
        result.status = OcrResult::Status::EngineError;
        return result;
    }

    const POINT probe{ screenRect.center().x(), screenRect.center().y() };

    // Several windows cover any given point, so each is tried in turn and
    // the first one whose text is genuinely located inside the selection
    // wins. Candidates without a text provider fail immediately, so the
    // usual cost is one attempt.
    for (HWND window : candidateWindowsAt(probe, MaxCandidates)) {
        if (readFromWindow(automation.Get(), window, screenRect, result)) {
            return result;
        }
    }

    return result;
}
