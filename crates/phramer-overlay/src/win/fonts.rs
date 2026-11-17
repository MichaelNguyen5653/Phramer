//! System fonts for the text tool's `fontFamily`, read through GDI: the
//! font GDI picks for a family and style, as the bytes of its file. GDI
//! does the matching (and refuses nothing), so the face it hands back is
//! checked against the family asked for; its real style is read from the
//! font itself, and `phramer-annotate` makes up the styles a family lacks.

use std::ptr::null_mut;

use ttf_parser::{Face, RawFace, Tag};
use windows_sys::Win32::Foundation::LPARAM;
use windows_sys::Win32::Graphics::Gdi::{
    CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
    DEFAULT_QUALITY, DeleteDC, DeleteObject, EnumFontFamiliesExW, FW_BOLD, FW_NORMAL, GDI_ERROR,
    GetFontData, GetTextFaceW, HDC, LOGFONTW, OUT_TT_ONLY_PRECIS, SelectObject, TEXTMETRICW,
    TRUETYPE_FONTTYPE,
};

/// One face of a system font family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemFace {
    /// What the face itself says it is, not what was asked for.
    pub bold: bool,
    pub italic: bool,
    /// The font file: a single font, or a whole collection (`.ttc`).
    pub data: Vec<u8>,
    /// The face's index in `data`; 0 for a single font.
    pub index: u32,
}

/// `GetFontData`'s table tags are the four bytes read as a little-endian
/// number.
fn table_tag(tag: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*tag)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// A memory DC with one font selected; both freed on drop.
struct SelectedFont {
    dc: HDC,
    font: windows_sys::Win32::Graphics::Gdi::HFONT,
    previous: windows_sys::Win32::Graphics::Gdi::HGDIOBJ,
}

impl SelectedFont {
    fn new(family: &str, bold: bool, italic: bool) -> Option<Self> {
        let name = wide(family);
        // SAFETY: a memory DC compatible with the screen; deleted on drop.
        let dc = unsafe { CreateCompatibleDC(null_mut()) };
        if dc.is_null() {
            return None;
        }
        let weight = if bold { FW_BOLD } else { FW_NORMAL };
        // SAFETY: plain values and a NUL-terminated name that outlives the
        // call; the font is deleted on drop.
        let font = unsafe {
            CreateFontW(
                -64,
                0,
                0,
                0,
                weight as i32,
                u32::from(italic),
                0,
                0,
                u32::from(DEFAULT_CHARSET),
                u32::from(OUT_TT_ONLY_PRECIS),
                u32::from(CLIP_DEFAULT_PRECIS),
                u32::from(DEFAULT_QUALITY),
                u32::from(DEFAULT_PITCH),
                name.as_ptr(),
            )
        };
        if font.is_null() {
            // SAFETY: created above, deleted once.
            unsafe { DeleteDC(dc) };
            return None;
        }
        // SAFETY: both handles are valid; the previous object is put back on
        // drop.
        let previous = unsafe { SelectObject(dc, font) };
        Some(Self { dc, font, previous })
    }

    /// The face name GDI actually chose.
    fn face_name(&self) -> String {
        let mut name = [0u16; 64];
        // SAFETY: `name` holds the 64 characters given.
        let length = unsafe { GetTextFaceW(self.dc, name.len() as i32, name.as_mut_ptr()) };
        let end = usize::try_from(length).unwrap_or(0).min(name.len());
        String::from_utf16_lossy(&name[..end])
            .trim_end_matches('\0')
            .to_owned()
    }

    /// A table of the selected font (`tag`), or the whole file for 0; `None`
    /// when there is none.
    fn data(&self, tag: u32) -> Option<Vec<u8>> {
        // SAFETY: a size query with no buffer.
        let size = unsafe { GetFontData(self.dc, tag, 0, null_mut(), 0) };
        if size == GDI_ERROR as u32 || size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        // SAFETY: `data` holds the `size` bytes asked for.
        let read = unsafe { GetFontData(self.dc, tag, 0, data.as_mut_ptr().cast(), size) };
        (read == size).then_some(data)
    }
}

impl Drop for SelectedFont {
    fn drop(&mut self) {
        // SAFETY: the handles made in `new`, each released once, the font
        // after it is deselected.
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.font);
            DeleteDC(self.dc);
        }
    }
}

/// The font file GDI picks for `family` in one style, as (data, index).
/// `None` when the family is not installed (GDI substituted another).
fn picked(family: &str, bold: bool, italic: bool) -> Option<(Vec<u8>, u32)> {
    let selected = SelectedFont::new(family, bold, italic)?;
    if !selected.face_name().eq_ignore_ascii_case(family) {
        return None;
    }
    // A collection is read whole; the face is the one whose `name` table
    // is the selected font's
    if let Some(collection) = selected.data(table_tag(b"ttcf")) {
        let name = selected.data(table_tag(b"name"))?;
        let count = ttf_parser::fonts_in_collection(&collection)?;
        let index = (0..count).find(|&index| {
            RawFace::parse(&collection, index)
                .ok()
                .and_then(|face| face.table(Tag::from_bytes(b"name")))
                .is_some_and(|table| table == name.as_slice())
        })?;
        return Some((collection, index));
    }
    Some((selected.data(0)?, 0))
}

/// The regular, bold, italic and bold italic faces of an installed family,
/// each once (a family without a bold face gives its regular face for bold,
/// which is listed once, as regular). Empty when the family is not
/// installed or is not TrueType or OpenType.
pub fn system_font_faces(family: &str) -> Vec<SystemFace> {
    let mut faces: Vec<SystemFace> = Vec::new();
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        let Some((data, index)) = picked(family, bold, italic) else {
            continue;
        };
        if faces
            .iter()
            .any(|face| face.index == index && face.data == data)
        {
            continue;
        }
        let Ok(parsed) = Face::parse(&data, index) else {
            continue;
        };
        faces.push(SystemFace {
            bold: parsed.is_bold(),
            italic: parsed.is_italic(),
            data,
            index,
        });
    }
    faces
}

unsafe extern "system" fn collect_family(
    font: *const LOGFONTW,
    _metrics: *const TEXTMETRICW,
    kind: u32,
    lparam: LPARAM,
) -> i32 {
    // SAFETY: `lparam` is the `&mut Vec` given to EnumFontFamiliesExW below,
    // alive for the whole enumeration, and `font` points to the font GDI is
    // reporting.
    let (names, font) = unsafe { (&mut *(lparam as *mut Vec<String>), &*font) };
    if kind & TRUETYPE_FONTTYPE != 0 {
        let end = font
            .lfFaceName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(font.lfFaceName.len());
        names.push(String::from_utf16_lossy(&font.lfFaceName[..end]));
    }
    1
}

/// The installed TrueType and OpenType families, sorted and each once,
/// without the `@` names GDI lists for vertical writing.
pub fn system_font_families() -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    // SAFETY: a memory DC, deleted below.
    let dc = unsafe { CreateCompatibleDC(null_mut()) };
    if dc.is_null() {
        return names;
    }
    let filter = LOGFONTW {
        lfCharSet: DEFAULT_CHARSET,
        ..Default::default()
    };
    // SAFETY: `filter` and `names` outlive the call; `collect_family` only
    // pushes into `names`.
    unsafe {
        EnumFontFamiliesExW(
            dc,
            &filter,
            Some(collect_family),
            &mut names as *mut Vec<String> as LPARAM,
            0,
        );
        DeleteDC(dc);
    }
    names.retain(|name| !name.is_empty() && !name.starts_with('@'));
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_tags_read_as_gdi_expects() {
        // 'name' is 0x656d616e for GetFontData
        assert_eq!(table_tag(b"name"), 0x656d_616e);
        assert_eq!(table_tag(b"ttcf"), 0x6663_7474);
    }
}
