//! 15.0.0's "Quit Capture" question (`CaptureWidget::promptQuit`): Yes/No,
//! No by default, with "Do not show this again".

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Controls::{
    TASKDIALOGCONFIG, TDCBF_NO_BUTTON, TDCBF_YES_BUTTON, TDF_POSITION_RELATIVE_TO_WINDOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDNO, IDYES, MB_DEFBUTTON2, MB_ICONQUESTION, MB_YESNO, MessageBoxW,
};
use windows_sys::core::{BOOL, HRESULT};

const TITLE: &str = "Quit Capture";
const QUESTION: &str = "Are you sure you want to quit capture?";
const STOP_ASKING: &str = "Do not show this again";

type TaskDialogIndirect =
    unsafe extern "system" fn(*const TASKDIALOGCONFIG, *mut i32, *mut i32, *mut BOOL) -> HRESULT;

/// Asks over `owner`; (quit, stop asking). Blocks in a modal loop, so it must
/// run outside the shell's borrow.
pub(super) fn ask_quit(owner: HWND) -> (bool, bool) {
    let (title, question, stop_asking) = (wide(TITLE), wide(QUESTION), wide(STOP_ASKING));
    // TaskDialogIndirect is only in Common Controls 6, which the app's
    // manifest selects; a process without it (the test binaries) would not
    // even load with a static import, so it is looked up, and a plain
    // message box without the check box stands in
    if let Some(task_dialog) = task_dialog_indirect() {
        let config = TASKDIALOGCONFIG {
            cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
            hwndParent: owner,
            dwFlags: TDF_POSITION_RELATIVE_TO_WINDOW,
            dwCommonButtons: TDCBF_YES_BUTTON | TDCBF_NO_BUTTON,
            pszWindowTitle: title.as_ptr(),
            pszMainInstruction: question.as_ptr(),
            nDefaultButton: IDNO,
            pszVerificationText: stop_asking.as_ptr(),
            ..Default::default()
        };
        let (mut button, mut checked) = (0i32, 0 as BOOL);
        // SAFETY: `config` and the strings it points to outlive the call;
        // the out-parameters are valid; the radio-button one may be null.
        let result =
            unsafe { task_dialog(&config, &mut button, std::ptr::null_mut(), &mut checked) };
        if result >= 0 {
            return (button == IDYES, checked != 0);
        }
        log::warn!("quit prompt: TaskDialogIndirect failed ({result:#x})");
    }
    // SAFETY: NUL-terminated strings that outlive the call.
    let answer = unsafe {
        MessageBoxW(
            owner,
            question.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
        )
    };
    (answer == IDYES, false)
}

fn task_dialog_indirect() -> Option<TaskDialogIndirect> {
    let library = wide("comctl32.dll");
    // SAFETY: a NUL-terminated name; comctl32 stays loaded for the process,
    // so the pointer stays valid.
    let module = unsafe { LoadLibraryW(library.as_ptr()) };
    if module.is_null() {
        return None;
    }
    // SAFETY: a loaded module and a NUL-terminated ANSI name.
    let address = unsafe { GetProcAddress(module, c"TaskDialogIndirect".as_ptr().cast()) }?;
    // SAFETY: TaskDialogIndirect has exactly this signature.
    Some(unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, TaskDialogIndirect>(address)
    })
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "live: shows the quit prompt and waits for an answer"]
    fn the_quit_prompt_answers() {
        let (quit, stop) = super::ask_quit(std::ptr::null_mut());
        println!("quit: {quit}, stop asking: {stop}");
    }
}
