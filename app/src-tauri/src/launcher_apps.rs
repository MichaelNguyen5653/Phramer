//! The apps App Launcher offers: 15.0.0's `WinLnkFileParser`. Every `.lnk`
//! in the per-user and all-users Start Menu whose target is an existing
//! `.exe`, except uninstallers and the system folders 15.0.0 skipped; one
//! entry per program. Programs Windows offers for image files (the
//! `OpenWithList` of each image extension) are "Graphics", the rest
//! "Utility". 15.0.0 listed no Store apps (they have no `.lnk`); neither does
//! this.
//!
//! Reading a few hundred shortcuts and their icons takes a while, so both
//! run on worker threads and are cached for the daemon's life (see
//! `launcher_ui`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

/// Start Menu folders 15.0.0 left out: whole words anywhere in the path.
static MENU_FILTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(Accessibility|Administrative Tools|Setup|System Tools|Uninstall|Update|Updater|Windows PowerShell)\b",
    )
    .expect("a valid expression")
});

/// The image formats 15.0.0 could write (`QImageWriter::supportedImageFormats`
/// with its Windows plugins), whose "Open with" programs count as Graphics.
pub(crate) const IMAGE_EXTENSIONS: &[&str] = &[
    "bmp", "cur", "ico", "jfif", "jpeg", "jpg", "pbm", "pgm", "png", "ppm", "tif", "tiff", "wbmp",
    "webp", "xbm", "xpm",
];

/// One program in the list.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AppEntry {
    /// The shortcut's name.
    pub(crate) name: String,
    /// The program it starts.
    pub(crate) exe: String,
    /// Offered for image files (15.0.0's Graphics tab), else Utility.
    pub(crate) graphics: bool,
}

/// Whether a shortcut at `path` is in a folder 15.0.0 skipped.
pub(crate) fn is_skipped_folder(path: &str) -> bool {
    MENU_FILTER.is_match(path)
}

/// Whether `target` is a program worth listing: an `.exe`, not an
/// uninstaller. (Whether it exists is checked by the caller.)
pub(crate) fn is_listable_target(target: &Path) -> bool {
    let is_exe = target
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
    let stem = target
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    is_exe && !stem.contains("unins")
}

/// The name shown: the shortcut's file name without `.lnk`.
pub(crate) fn shortcut_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The list from `(name, exe)` pairs in discovery order (per-user Start
/// Menu first): the first shortcut to a program wins, Graphics before
/// Utility, each sorted by name. `graphics` holds lower-case file names.
pub(crate) fn build_list(
    found: Vec<(String, String)>,
    graphics: &HashSet<String>,
) -> Vec<AppEntry> {
    let mut seen = HashSet::new();
    let mut apps: Vec<AppEntry> = found
        .into_iter()
        .filter(|(name, exe)| !name.is_empty() && !exe.is_empty())
        .filter(|(_, exe)| seen.insert(exe.to_lowercase()))
        .map(|(name, exe)| {
            let file = Path::new(&exe)
                .file_name()
                .map(|file| file.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            AppEntry {
                graphics: graphics.contains(&file),
                name,
                exe,
            }
        })
        .collect();
    apps.sort_by(|a, b| {
        b.graphics
            .cmp(&a.graphics)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    apps
}

/// The `OpenWithList` entries 15.0.0 read: single-letter value names whose
/// data is a program file name, lower-cased.
pub(crate) fn open_with_programs<'a>(
    values: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<String> {
    values
        .into_iter()
        .filter(|(name, data)| name.chars().count() == 1 && data.to_lowercase().ends_with(".exe"))
        .map(|(_, data)| data.to_lowercase())
        .collect()
}

/// Every `.lnk` under `folder`, at any depth, in a stable order.
fn shortcuts_in(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => shortcuts_in(&path, found),
            Ok(_)
                if path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk")) =>
            {
                found.push(path);
            }
            _ => {}
        }
    }
}

/// The installed programs. Slow: call it from a worker thread.
pub(crate) fn enumerate() -> Vec<AppEntry> {
    let started = std::time::Instant::now();
    let mut shortcuts = Vec::new();
    for folder in win::start_menu_folders() {
        shortcuts_in(&folder, &mut shortcuts);
    }
    let found: Vec<(String, String)> = {
        let _apartment = win::Apartment::enter();
        shortcuts
            .iter()
            .filter(|path| !is_skipped_folder(&path.to_string_lossy()))
            .filter_map(|path| {
                let target = win::shortcut_target(path)?;
                (is_listable_target(&target) && target.is_file())
                    .then(|| (shortcut_name(path), target.to_string_lossy().into_owned()))
            })
            .collect()
    };
    let graphics: HashSet<String> = IMAGE_EXTENSIONS
        .iter()
        .flat_map(|extension| win::open_with_list(extension))
        .collect();
    let apps = build_list(found, &graphics);
    log::info!(
        "launcher: {} apps from {} shortcuts in {} ms",
        apps.len(),
        shortcuts.len(),
        started.elapsed().as_millis()
    );
    apps
}

/// `exe`'s icon as a PNG data URL, `None` when it has none.
pub(crate) fn icon_data_url(exe: &str) -> Option<String> {
    let png = win::icon_png(exe)?;
    Some(format!("data:image/png;base64,{}", base64(&png)))
}

/// Standard base64 with padding.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for index in 0..4 {
            if index <= chunk.len() {
                text.push(char::from(
                    ALPHABET[(value >> (18 - 6 * index) & 63) as usize],
                ));
            } else {
                text.push('=');
            }
        }
    }
    text
}

/// Straight BGRA from an icon's colour and mask bitmaps: icons without
/// alpha take it from the mask.
pub(crate) fn icon_alpha(bgra: &mut [u8], mask_transparent: impl Fn(usize) -> bool) {
    let pixels = bgra.len() / 4;
    if (0..pixels).any(|index| bgra[index * 4 + 3] != 0) {
        return;
    }
    for index in 0..pixels {
        bgra[index * 4 + 3] = if mask_transparent(index) { 0 } else { 255 };
    }
}

mod win {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{Interface, PCWSTR};
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW, HBITMAP,
    };
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, REG_SZ, RegCloseKey, RegEnumValueW, RegOpenKeyExW,
    };
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_CommonPrograms, FOLDERID_Programs, SHDefExtractIconW, SHFILEINFOW, SHGFI_ICON,
        SHGFI_LARGEICON, SHGetFileInfoW, SHGetKnownFolderPath,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    /// Icons are drawn at up to 32 px; 64 keeps them sharp at 200%.
    const ICON_SIZE: u32 = 64;
    /// `SLGP_UNCPRIORITY`, as Qt asks for a shortcut's target.
    const SLGP_UNCPRIORITY: u32 = 2;

    /// A single-threaded apartment for the shell calls, left on drop.
    pub(super) struct Apartment(bool);

    impl Apartment {
        pub(super) fn enter() -> Self {
            // SAFETY: no pointers; balanced by the drop below when it succeeds.
            Self(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
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

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    /// The per-user Start Menu first, then the all-users one.
    pub(super) fn start_menu_folders() -> Vec<PathBuf> {
        [&FOLDERID_Programs, &FOLDERID_CommonPrograms]
            .into_iter()
            .filter_map(|id| {
                let mut path = std::ptr::null_mut();
                // SAFETY: `path` receives a string freed below, even on failure.
                let result =
                    unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut path) };
                let folder = (result >= 0 && !path.is_null()).then(|| {
                    // SAFETY: a NUL-terminated string from the shell.
                    let length = (0..)
                        .take_while(|&at| unsafe { *path.add(at) } != 0)
                        .count();
                    // SAFETY: `length` characters precede the NUL.
                    PathBuf::from(std::ffi::OsString::from_wide(unsafe {
                        std::slice::from_raw_parts(path, length)
                    }))
                });
                // SAFETY: allocated by SHGetKnownFolderPath; null is allowed.
                unsafe { CoTaskMemFree(path.cast()) };
                folder
            })
            .collect()
    }

    /// Where a shortcut points (`QFileInfo::symLinkTarget`).
    pub(super) fn shortcut_target(path: &Path) -> Option<PathBuf> {
        let file = wide(path);
        // SAFETY: COM calls in this thread's apartment; `file` and `target`
        // outlive the calls that use them.
        unsafe {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
            link.cast::<IPersistFile>()
                .ok()?
                .Load(PCWSTR(file.as_ptr()), STGM_READ)
                .ok()?;
            let mut target = [0u16; 1024];
            link.GetPath(&mut target, std::ptr::null_mut(), SLGP_UNCPRIORITY)
                .ok()?;
            let length = target.iter().position(|&c| c == 0).unwrap_or(target.len());
            (length > 0).then(|| PathBuf::from(std::ffi::OsString::from_wide(&target[..length])))
        }
    }

    /// The program file names under
    /// `HKCU\...\Explorer\FileExts\.<extension>\OpenWithList`.
    pub(super) fn open_with_list(extension: &str) -> Vec<String> {
        let key: Vec<u16> = format!(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\.{extension}\\OpenWithList"
        )
        .encode_utf16()
        .chain(Some(0))
        .collect();
        let mut handle: HKEY = std::ptr::null_mut();
        // SAFETY: `key` is NUL-terminated; `handle` is closed below.
        if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, KEY_READ, &mut handle) }
            != ERROR_SUCCESS
        {
            return Vec::new();
        }
        let mut values: Vec<(String, String)> = Vec::new();
        for index in 0.. {
            let mut name = [0u16; 256];
            let mut name_length = name.len() as u32;
            let mut data = [0u16; 1024];
            let mut data_bytes = (data.len() * 2) as u32;
            let mut kind = 0;
            // SAFETY: the buffers hold the sizes given; `handle` is open.
            let status = unsafe {
                RegEnumValueW(
                    handle,
                    index,
                    name.as_mut_ptr(),
                    &mut name_length,
                    std::ptr::null(),
                    &mut kind,
                    data.as_mut_ptr().cast(),
                    &mut data_bytes,
                )
            };
            if status != ERROR_SUCCESS {
                break;
            }
            if kind != REG_SZ {
                continue;
            }
            let data_length = (data_bytes as usize / 2).min(data.len());
            let text = String::from_utf16_lossy(&data[..data_length]);
            values.push((
                String::from_utf16_lossy(&name[..name_length as usize]),
                text.trim_end_matches('\0').to_owned(),
            ));
        }
        // SAFETY: opened above, closed once.
        unsafe { RegCloseKey(handle) };
        super::open_with_programs(
            values
                .iter()
                .map(|(name, data)| (name.as_str(), data.as_str())),
        )
    }

    /// `exe`'s icon as a PNG.
    pub(super) fn icon_png(exe: &str) -> Option<Vec<u8>> {
        let _apartment = Apartment::enter();
        let file: Vec<u16> = exe.encode_utf16().chain(Some(0)).collect();
        let mut icon: HICON = std::ptr::null_mut();
        // SAFETY: `file` is NUL-terminated; the icon is destroyed below.
        let extracted = unsafe {
            SHDefExtractIconW(
                file.as_ptr(),
                0,
                0,
                &mut icon,
                std::ptr::null_mut(),
                ICON_SIZE,
            )
        };
        if extracted != 0 || icon.is_null() {
            // No icon of its own: the shell's (the generic program icon)
            // SAFETY: an all-zero SHFILEINFOW is valid; `file` is NUL-terminated.
            let mut info: SHFILEINFOW = unsafe { std::mem::zeroed() };
            // SAFETY: `info` is the size given.
            let found = unsafe {
                SHGetFileInfoW(
                    file.as_ptr(),
                    0,
                    &mut info,
                    size_of::<SHFILEINFOW>() as u32,
                    SHGFI_ICON | SHGFI_LARGEICON,
                )
            };
            if found == 0 || info.hIcon.is_null() {
                return None;
            }
            icon = info.hIcon;
        }
        let png = icon_to_png(icon);
        // SAFETY: an icon this function obtained, destroyed once.
        unsafe { DestroyIcon(icon) };
        png
    }

    /// A bitmap's pixels as top-down 32-bit BGRA, with its size.
    fn bitmap_bgra(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
        // SAFETY: an all-zero BITMAP is valid; GetObjectW fills it.
        let mut header: BITMAP = unsafe { std::mem::zeroed() };
        // SAFETY: `header` is the size given.
        if unsafe { GetObjectW(bitmap, size_of::<BITMAP>() as i32, (&raw mut header).cast()) } == 0
        {
            return None;
        }
        let (width, height) = (header.bmWidth.max(0) as u32, header.bmHeight.max(0) as u32);
        if width == 0 || height == 0 {
            return None;
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // Negative: top-down rows
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        // SAFETY: a memory DC deleted below; `pixels` holds the rows asked for.
        let rows = unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            let rows = GetDIBits(
                dc,
                bitmap,
                0,
                height,
                pixels.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            );
            DeleteDC(dc);
            rows
        };
        (rows > 0).then_some((width, height, pixels))
    }

    fn icon_to_png(icon: HICON) -> Option<Vec<u8>> {
        // SAFETY: an all-zero ICONINFO is valid; GetIconInfo fills it.
        let mut info: ICONINFO = unsafe { std::mem::zeroed() };
        // SAFETY: `icon` is a live icon; its bitmaps are deleted below.
        if unsafe { GetIconInfo(icon, &mut info) } == 0 {
            return None;
        }
        let color = bitmap_bgra(info.hbmColor);
        let mask = bitmap_bgra(info.hbmMask);
        // SAFETY: bitmaps GetIconInfo created, deleted once (null is ignored).
        unsafe {
            DeleteObject(info.hbmColor);
            DeleteObject(info.hbmMask);
        }
        let (width, height, mut bgra) = color?;
        super::icon_alpha(&mut bgra, |index| {
            mask.as_ref()
                .is_some_and(|(_, _, mask)| mask.get(index * 4).is_some_and(|value| *value != 0))
        });
        // BGRA to RGBA
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&bgra).ok()?;
        writer.finish().ok()?;
        Some(png)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_folders_are_skipped_by_whole_word() {
        let menu = r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs";
        assert!(is_skipped_folder(&format!(
            r"{menu}\Administrative Tools\Services.lnk"
        )));
        assert!(is_skipped_folder(&format!(
            r"{menu}\Accessibility\Magnify.lnk"
        )));
        assert!(is_skipped_folder(&format!(
            r"{menu}\Windows PowerShell\Windows PowerShell.lnk"
        )));
        assert!(is_skipped_folder(&format!(r"{menu}\Foo\Uninstall Foo.lnk")));
        assert!(is_skipped_folder(&format!(r"{menu}\Foo\Foo Updater.lnk")));
        assert!(!is_skipped_folder(&format!(r"{menu}\Paint.NET.lnk")));
        // Words, not fragments
        assert!(!is_skipped_folder(&format!(
            r"{menu}\Updates Viewer\Thing.lnk"
        )));
        assert!(!is_skipped_folder(&format!(r"{menu}\Setups\Thing.lnk")));
    }

    #[test]
    fn only_programs_that_are_not_uninstallers_are_listed() {
        assert!(is_listable_target(Path::new(
            r"C:\Windows\System32\notepad.exe"
        )));
        assert!(is_listable_target(Path::new(r"C:\Apps\GIMP.EXE")));
        assert!(!is_listable_target(Path::new(r"C:\Apps\unins000.exe")));
        assert!(!is_listable_target(Path::new(r"C:\Apps\Uninstall.exe")));
        assert!(!is_listable_target(Path::new(r"C:\Apps\readme.txt")));
        assert!(!is_listable_target(Path::new(r"C:\Apps")));
    }

    #[test]
    fn names_are_the_shortcut_names() {
        assert_eq!(
            shortcut_name(Path::new(r"C:\Menu\Paint.NET.lnk")),
            "Paint.NET"
        );
        assert_eq!(
            shortcut_name(Path::new(r"C:\Menu\Notepad++.lnk")),
            "Notepad++"
        );
    }

    #[test]
    fn the_list_is_deduplicated_grouped_and_sorted() {
        let graphics: HashSet<String> = ["mspaint.exe".to_owned(), "gimp.exe".to_owned()].into();
        let list = build_list(
            vec![
                ("zed".into(), r"C:\z\zed.exe".into()),
                ("GIMP".into(), r"C:\g\gimp.exe".into()),
                ("notepad".into(), r"C:\Windows\notepad.exe".into()),
                // A second shortcut to the same program: the first wins
                ("Notepad (copy)".into(), r"C:\WINDOWS\NOTEPAD.EXE".into()),
                ("Paint".into(), r"C:\Windows\MSPaint.exe".into()),
                ("".into(), r"C:\x\nameless.exe".into()),
                ("Alpha".into(), r"C:\a\alpha.exe".into()),
            ],
            &graphics,
        );
        let names: Vec<&str> = list.iter().map(|app| app.name.as_str()).collect();
        assert_eq!(names, ["GIMP", "Paint", "Alpha", "notepad", "zed"]);
        assert!(list[0].graphics && list[1].graphics && !list[2].graphics);
    }

    #[test]
    fn open_with_lists_keep_single_letter_programs() {
        let programs = open_with_programs([
            ("a", "mspaint.exe"),
            ("b", "Photos.EXE"),
            ("MRUList", "ba"),
            ("c", "something.dll"),
        ]);
        assert_eq!(programs, ["mspaint.exe", "photos.exe"]);
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    }

    #[test]
    fn icons_without_alpha_take_it_from_the_mask() {
        let mut bgra = vec![10, 20, 30, 0, 40, 50, 60, 0];
        icon_alpha(&mut bgra, |index| index == 1);
        assert_eq!(bgra, [10, 20, 30, 255, 40, 50, 60, 0]);
        // Alpha present: left alone
        let mut with_alpha = vec![1, 2, 3, 128, 4, 5, 6, 0];
        icon_alpha(&mut with_alpha, |_| true);
        assert_eq!(with_alpha, [1, 2, 3, 128, 4, 5, 6, 0]);
    }

    /// Reads the real Start Menu and icons: Windows Sandbox only.
    #[test]
    #[ignore = "live: reads this Windows' Start Menu, registry and icons"]
    fn the_start_menu_is_listed_with_icons() {
        let folders = win::start_menu_folders();
        println!("Start Menu folders: {folders:?}");
        assert_eq!(folders.len(), 2, "per-user and all-users");
        let apps = enumerate();
        for app in &apps {
            println!(
                "  {} [{}] {}",
                app.name,
                if app.graphics { "Graphics" } else { "Utility" },
                app.exe
            );
        }
        assert!(!apps.is_empty(), "no apps listed");
        let mut exes = HashSet::new();
        for app in &apps {
            assert!(Path::new(&app.exe).is_file(), "{} is not a file", app.exe);
            assert!(is_listable_target(Path::new(&app.exe)));
            assert!(
                exes.insert(app.exe.to_lowercase()),
                "{} listed twice",
                app.exe
            );
        }
        let started = std::time::Instant::now();
        let with_icons = apps
            .iter()
            .filter(|app| {
                icon_data_url(&app.exe)
                    .is_some_and(|url| url.starts_with("data:image/png;base64,iVBOR"))
            })
            .count();
        println!(
            "{with_icons} of {} icons as PNG in {} ms",
            apps.len(),
            started.elapsed().as_millis()
        );
        assert_eq!(
            with_icons,
            apps.len(),
            "every program has an icon (its own or the shell's)"
        );
    }
}
