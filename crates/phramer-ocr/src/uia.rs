//! The selection's text read straight from the windows under it through UI
//! Automation (`uiatextreader.cpp`): exact where recognition guesses.
//!
//! A window covering the selection is only a candidate: overlapping windows
//! are the normal case, and the z-order has moved on since the capture
//! froze the screen. A candidate is accepted only when the text it returns
//! is positioned inside the selection, and only lines wholly inside count,
//! so words the selection clipped never come back. [`collect_lines`] is that
//! walk over a provider's lines; it is pure and works on any [`LineCursor`].

use crate::{OcrLine, Rect};

/// Upper bound on the line walk; reached only if a provider reports lines
/// that do not advance.
pub const MAX_LINES: usize = 500;
/// Physical pixels a line's box may extend past the selection and still
/// count as inside it: providers pad line boxes past the glyphs.
pub const LINE_BOX_TOLERANCE: f64 = 3.0;

/// A text range walked line by line, starting at the selection's top.
pub trait LineCursor {
    /// The union of the line's rectangles in physical screen pixels; `None`
    /// when the provider reports none (scrolled out of view).
    fn bounds(&mut self) -> Option<Rect>;
    /// The line's text, `None` when it cannot be read.
    fn text(&mut self) -> Option<String>;
    /// Moves to the next line; false at the end.
    fn advance(&mut self) -> bool;
}

/// The union of the rectangles `GetBoundingRectangles` returns: four
/// doubles each (left, top, width, height); empty ones ignored.
pub fn union_of_rects(values: &[f64]) -> Option<Rect> {
    values
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&[x, y, width, height]| Rect::new(x, y, width, height))
        .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
        .reduce(|a, b| a.united(&b))
}

/// The lines wholly inside `selection`, in order, with line breaks removed.
/// Stops at the first line below the selection: providers expose the
/// scrollback beyond the viewport too.
pub fn collect_lines(cursor: &mut impl LineCursor, selection: Rect) -> Vec<OcrLine> {
    let tolerant = selection.expanded(LINE_BOX_TOLERANCE);
    let mut lines = Vec::new();
    for _ in 0..MAX_LINES {
        let bounds = cursor.bounds();
        if bounds.is_some_and(|bounds| bounds.y > selection.bottom()) {
            break;
        }
        // Contained, not merely touching: a clipped line holds words that
        // were never in the capture. Dropping it lets recognition answer.
        if let Some(bounds) = bounds
            && tolerant.contains(&bounds)
            && let Some(text) = cursor.text()
        {
            let text: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
            if !text.trim().is_empty() {
                lines.push(OcrLine {
                    text,
                    bounding_box: bounds,
                    words: Vec::new(),
                });
            }
        }
        if !cursor.advance() {
            break;
        }
    }
    lines
}

#[cfg(windows)]
pub use win::read_window_text;

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::{HWND, POINT, RECT};
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::System::Ole::{
        SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound,
        SafeArrayUnaccessData,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement,
        IUIAutomationTextPattern, IUIAutomationTextRange, TextUnit_Line, TreeScope_Descendants,
        UIA_IsPasswordPropertyId, UIA_IsTextPatternAvailablePropertyId, UIA_TextPatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GW_HWNDNEXT, GetTopWindow, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible,
    };
    use windows::core::Interface;

    use super::{LineCursor, collect_lines, union_of_rects};
    use crate::{OcrResult, OcrStatus, Rect};

    /// A provider that misbehaves must not hang the worker; generous enough
    /// that a busy but healthy application still answers.
    const CONNECTION_TIMEOUT_MS: u32 = 1000;
    const TRANSACTION_TIMEOUT_MS: u32 = 3000;
    /// Overlapping windows tried before giving up. Windows without a text
    /// provider are rejected in a call or two.
    const MAX_CANDIDATES: usize = 6;

    /// A multithreaded apartment for this call, left again on drop unless
    /// the thread already had one of another kind.
    struct Apartment(bool);

    impl Apartment {
        fn enter() -> Self {
            // SAFETY: no pointers; balanced by the drop below when it succeeds.
            Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
        }
    }

    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: balances the successful CoInitializeEx above.
                unsafe { CoUninitialize() };
            }
        }
    }

    fn is_cloaked(window: HWND) -> bool {
        // Suspended UWP apps and windows on other virtual desktops are
        // invisible but pass IsWindowVisible
        let mut cloaked = 0u32;
        // SAFETY: `cloaked` is a u32 out-parameter of the size given.
        unsafe {
            DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED,
                (&raw mut cloaked).cast(),
                size_of::<u32>() as u32,
            )
        }
        .is_ok()
            && cloaked != 0
    }

    /// Top-level windows whose rectangle covers `point`, topmost first, not
    /// this process's. WindowFromPoint would only ever find our own.
    fn candidate_windows_at(point: POINT) -> Vec<HWND> {
        let own = std::process::id();
        let mut candidates = Vec::new();
        // SAFETY: plain queries on window handles; a stale one just fails.
        unsafe {
            let mut window = GetTopWindow(None).ok();
            while let Some(current) = window
                && candidates.len() < MAX_CANDIDATES
            {
                window = GetWindow(current, GW_HWNDNEXT).ok();
                let mut process = 0;
                GetWindowThreadProcessId(current, Some(&mut process));
                if process == own
                    || !IsWindowVisible(current).as_bool()
                    || IsIconic(current).as_bool()
                {
                    continue;
                }
                let mut bounds = RECT::default();
                if GetWindowRect(current, &mut bounds).is_err()
                    || point.x < bounds.left
                    || point.x >= bounds.right
                    || point.y < bounds.top
                    || point.y >= bounds.bottom
                    || is_cloaked(current)
                {
                    continue;
                }
                candidates.push(current);
            }
        }
        candidates
    }

    fn automation() -> Option<IUIAutomation> {
        // SAFETY: COM creation in this thread's apartment.
        unsafe {
            // CUIAutomation8 adds the timeouts; the plain one for older systems
            if let Ok(automation) =
                CoCreateInstance::<_, IUIAutomation2>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
            {
                let _ = automation.SetConnectionTimeout(CONNECTION_TIMEOUT_MS);
                let _ = automation.SetTransactionTimeout(TRANSACTION_TIMEOUT_MS);
                // Reading text must never take the focus
                let _ = automation.SetAutoSetFocus(false);
                return automation.cast().ok();
            }
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()
        }
    }

    /// The element itself when it exposes text, otherwise its first
    /// descendant that does.
    fn text_element(
        automation: &IUIAutomation,
        root: &IUIAutomationElement,
    ) -> Option<IUIAutomationElement> {
        // SAFETY: COM calls on live interfaces.
        unsafe {
            if root
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .is_ok()
            {
                return Some(root.clone());
            }
            let condition = automation
                .CreatePropertyCondition(UIA_IsTextPatternAvailablePropertyId, &VARIANT::from(true))
                .ok()?;
            root.FindFirst(TreeScope_Descendants, &condition).ok()
        }
    }

    struct UiaCursor(IUIAutomationTextRange);

    impl LineCursor for UiaCursor {
        fn bounds(&mut self) -> Option<Rect> {
            // SAFETY: the array comes from the call and is destroyed once;
            // its data is read only between access and unaccess.
            unsafe {
                let array = self.0.GetBoundingRectangles().ok()?;
                if array.is_null() {
                    return None;
                }
                let mut values: Vec<f64> = Vec::new();
                let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
                if SafeArrayAccessData(array, &mut data).is_ok() {
                    let lower = SafeArrayGetLBound(array, 1).unwrap_or(0);
                    let upper = SafeArrayGetUBound(array, 1).unwrap_or(-1);
                    let count = usize::try_from(upper - lower + 1).unwrap_or(0);
                    if !data.is_null() {
                        values = std::slice::from_raw_parts(data.cast::<f64>(), count).to_vec();
                    }
                    let _ = SafeArrayUnaccessData(array);
                }
                let _ = SafeArrayDestroy(array);
                union_of_rects(&values)
            }
        }

        fn text(&mut self) -> Option<String> {
            // SAFETY: a COM call on a live range.
            unsafe { self.0.GetText(-1) }
                .ok()
                .map(|text| text.to_string())
        }

        fn advance(&mut self) -> bool {
            // SAFETY: a COM call on a live range.
            unsafe { self.0.Move(TextUnit_Line, 1) }.is_ok_and(|moved| moved != 0)
        }
    }

    /// The selection's lines from one candidate window, or `None` when it
    /// has no readable text inside the selection.
    fn read_from_window(
        automation: &IUIAutomation,
        window: HWND,
        selection: Rect,
    ) -> Option<Vec<crate::OcrLine>> {
        // SAFETY: COM calls on live interfaces.
        unsafe {
            let root = automation.ElementFromHandle(window).ok()?;
            let element = text_element(automation, &root)?;
            // Never a password box, whatever it shows: a control that masks
            // its pixels but not its provider would hand the secret over
            let password = element
                .GetCurrentPropertyValue(UIA_IsPasswordPropertyId)
                .ok()?;
            if bool::try_from(&password).unwrap_or(false) {
                return None;
            }
            let pattern = element
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .ok()?;
            // From the selection's top keeps the walk short however much
            // scrollback the provider holds
            let start = POINT {
                x: (selection.x + selection.width / 2.0) as i32,
                y: selection.y as i32 + 1,
            };
            let range = match pattern.RangeFromPoint(start) {
                Ok(range) => range,
                Err(_) => pattern.DocumentRange().ok()?,
            };
            let _ = range.ExpandToEnclosingUnit(TextUnit_Line);
            let lines = collect_lines(&mut UiaCursor(range), selection);
            (!lines.is_empty()).then_some(lines)
        }
    }

    /// The text of the window under `selection` (physical screen pixels),
    /// `NoTextFound` when none yields text inside it.
    pub fn read_window_text(selection: Rect) -> OcrResult {
        if selection.width <= 0.0 || selection.height <= 0.0 {
            return OcrResult::with_status(OcrStatus::NoTextFound);
        }
        let _apartment = Apartment::enter();
        let Some(automation) = automation() else {
            return OcrResult::with_status(OcrStatus::EngineError(String::new()));
        };
        let probe = POINT {
            x: (selection.x + selection.width / 2.0) as i32,
            y: (selection.y + selection.height / 2.0) as i32,
        };
        for window in candidate_windows_at(probe) {
            if let Some(lines) = read_from_window(&automation, window, selection) {
                return OcrResult::ok(lines);
            }
        }
        OcrResult::with_status(OcrStatus::NoTextFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A provider's lines as fixtures: each with its rectangles (as
    /// `GetBoundingRectangles` returns them) and its text.
    struct Fixture {
        lines: Vec<(Vec<f64>, &'static str)>,
        at: usize,
        reads: usize,
    }

    impl Fixture {
        fn new(lines: Vec<(Vec<f64>, &'static str)>) -> Self {
            Self {
                lines,
                at: 0,
                reads: 0,
            }
        }
    }

    impl LineCursor for Fixture {
        fn bounds(&mut self) -> Option<Rect> {
            union_of_rects(&self.lines[self.at].0)
        }
        fn text(&mut self) -> Option<String> {
            self.reads += 1;
            Some(self.lines[self.at].1.to_owned())
        }
        fn advance(&mut self) -> bool {
            if self.at + 1 < self.lines.len() {
                self.at += 1;
                true
            } else {
                false
            }
        }
    }

    /// The selection used throughout: x 100..500, y 200..300.
    const SELECTION: Rect = Rect::new(100.0, 200.0, 400.0, 100.0);

    #[test]
    fn rectangles_are_united_and_empty_ones_ignored() {
        assert_eq!(union_of_rects(&[]), None);
        assert_eq!(
            union_of_rects(&[
                10.0, 20.0, 30.0, 10.0, 0.0, 0.0, 0.0, 0.0, 50.0, 18.0, 10.0, 14.0
            ]),
            Some(Rect::new(10.0, 18.0, 50.0, 14.0))
        );
        // A trailing partial rectangle is not one
        assert_eq!(
            union_of_rects(&[1.0, 2.0, 3.0, 4.0, 9.0]),
            Some(Rect::new(1.0, 2.0, 3.0, 4.0))
        );
    }

    #[test]
    fn only_lines_inside_the_selection_come_back() {
        let mut cursor = Fixture::new(vec![
            // Above: clipped by the selection's top
            (vec![110.0, 190.0, 200.0, 18.0], "half seen\r\n"),
            (vec![110.0, 210.0, 200.0, 18.0], "first line\r\n"),
            // Padded two pixels past the edge: still inside
            (vec![98.0, 240.0, 404.0, 18.0], "second line"),
            // Running past the right edge: clipped words, dropped
            (vec![110.0, 262.0, 600.0, 18.0], "too wide for it"),
            // Two rectangles (a wrapped run), both inside
            (
                vec![110.0, 280.0, 100.0, 18.0, 250.0, 280.0, 100.0, 18.0],
                "third",
            ),
        ]);
        let lines = collect_lines(&mut cursor, SELECTION);
        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(texts, ["first line", "second line", "third"]);
        assert_eq!(lines[2].bounding_box, Rect::new(110.0, 280.0, 240.0, 18.0));
    }

    #[test]
    fn the_walk_stops_below_the_selection() {
        let mut cursor = Fixture::new(vec![
            (vec![110.0, 210.0, 200.0, 18.0], "inside"),
            (vec![110.0, 310.0, 200.0, 18.0], "below"),
            (vec![110.0, 230.0, 200.0, 18.0], "never reached"),
        ]);
        let lines = collect_lines(&mut cursor, SELECTION);
        assert_eq!(lines.len(), 1);
        assert_eq!(cursor.at, 1);
    }

    #[test]
    fn lines_without_boxes_or_text_are_skipped() {
        let mut cursor = Fixture::new(vec![
            // Scrolled out of view: no rectangles
            (vec![], "offscreen"),
            (vec![110.0, 210.0, 200.0, 18.0], "   \r\n"),
            (vec![110.0, 230.0, 200.0, 18.0], "kept"),
        ]);
        let lines = collect_lines(&mut cursor, SELECTION);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "kept");
        // The offscreen line's text was never asked for
        assert_eq!(cursor.reads, 2);
    }

    #[test]
    fn a_provider_that_never_ends_is_bounded() {
        struct Endless(usize);
        impl LineCursor for Endless {
            fn bounds(&mut self) -> Option<Rect> {
                Some(Rect::new(110.0, 210.0, 10.0, 10.0))
            }
            fn text(&mut self) -> Option<String> {
                Some("again".into())
            }
            fn advance(&mut self) -> bool {
                self.0 += 1;
                true
            }
        }
        let mut cursor = Endless(0);
        assert_eq!(collect_lines(&mut cursor, SELECTION).len(), MAX_LINES);
        assert_eq!(cursor.0, MAX_LINES);
    }
}
