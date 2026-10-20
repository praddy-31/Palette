//! Installed fonts, asked of macOS's font system (CoreText) directly: the
//! families, and the styles each one really has. SketchyBar finds a font by
//! family and style name (`JetBrainsMono Nerd Font:ExtraBold`); a style the
//! font doesn't have falls back to a system font, which has no icons.
//!
//! Plain C calls into two frameworks every Mac app already loads (no crate,
//! no program run), so a lookup costs a few milliseconds.

use std::ffi::{c_char, c_void};

type CFTypeRef = *const c_void;
type CFIndex = isize;

/// A C struct only ever passed by address.
#[repr(C)]
struct Opaque {
    _private: [u8; 0],
}

const UTF8: u32 = 0x0800_0100; // kCFStringEncodingUTF8

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeDictionaryKeyCallBacks: Opaque;
    static kCFTypeDictionaryValueCallBacks: Opaque;
    fn CFRelease(cf: CFTypeRef);
    fn CFArrayGetCount(array: CFTypeRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, i: CFIndex) -> CFTypeRef;
    fn CFStringCreateWithBytes(
        alloc: CFTypeRef,
        bytes: *const u8,
        len: CFIndex,
        encoding: u32,
        external: u8,
    ) -> CFTypeRef;
    fn CFStringGetLength(s: CFTypeRef) -> CFIndex;
    fn CFStringGetMaximumSizeForEncoding(len: CFIndex, encoding: u32) -> CFIndex;
    fn CFStringGetCString(s: CFTypeRef, buf: *mut c_char, size: CFIndex, encoding: u32) -> u8;
    fn CFDictionaryCreate(
        alloc: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        n: CFIndex,
        key_callbacks: *const Opaque,
        value_callbacks: *const Opaque,
    ) -> CFTypeRef;
}

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    static kCTFontFamilyNameAttribute: CFTypeRef;
    static kCTFontStyleNameAttribute: CFTypeRef;
    fn CTFontManagerCopyAvailableFontFamilyNames() -> CFTypeRef;
    fn CTFontDescriptorCreateWithAttributes(attributes: CFTypeRef) -> CFTypeRef;
    fn CTFontDescriptorCreateMatchingFontDescriptors(
        descriptor: CFTypeRef,
        mandatory: CFTypeRef,
    ) -> CFTypeRef;
    fn CTFontDescriptorCopyAttribute(descriptor: CFTypeRef, attribute: CFTypeRef) -> CFTypeRef;
}

/// Releases a CoreFoundation object it owns when dropped ("Create" and
/// "Copy" functions hand you one you must release).
struct Owned(CFTypeRef);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

fn cf_string(s: &str) -> Owned {
    Owned(unsafe {
        CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as CFIndex, UTF8, 0)
    })
}

fn rust_string(s: CFTypeRef) -> Option<String> {
    if s.is_null() {
        return None;
    }
    unsafe {
        let size = CFStringGetMaximumSizeForEncoding(CFStringGetLength(s), UTF8) + 1;
        let mut buf = vec![0u8; size as usize];
        if CFStringGetCString(s, buf.as_mut_ptr().cast(), size, UTF8) == 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        buf.truncate(end);
        String::from_utf8(buf).ok()
    }
}

/// The values of a CFArray the caller keeps alive (not owned).
fn items(array: &Owned) -> Vec<CFTypeRef> {
    if array.0.is_null() {
        return Vec::new();
    }
    let n = unsafe { CFArrayGetCount(array.0) };
    (0..n)
        .map(|i| unsafe { CFArrayGetValueAtIndex(array.0, i) })
        .collect()
}

/// Every font family installed, sorted (the names as Font Book shows them).
pub fn families() -> Vec<String> {
    let list = Owned(unsafe { CTFontManagerCopyAvailableFontFamilyNames() });
    let mut names: Vec<String> = items(&list)
        .into_iter()
        .filter_map(rust_string)
        .filter(|n| !n.starts_with('.')) // hidden system fonts
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// The styles a family has (`Regular`, `Bold`, `ExtraBold`, …), lightest
/// first, upright before italic. Empty when the family isn't installed.
pub fn styles(family: &str) -> Vec<String> {
    let key = unsafe { kCTFontFamilyNameAttribute };
    let name = cf_string(family);
    let attrs = Owned(unsafe {
        CFDictionaryCreate(
            std::ptr::null(),
            &key,
            &name.0,
            1,
            &raw const kCFTypeDictionaryKeyCallBacks,
            &raw const kCFTypeDictionaryValueCallBacks,
        )
    });
    let wanted = Owned(unsafe { CTFontDescriptorCreateWithAttributes(attrs.0) });
    let found =
        Owned(unsafe { CTFontDescriptorCreateMatchingFontDescriptors(wanted.0, std::ptr::null()) });
    let mut out: Vec<String> = Vec::new();
    for d in items(&found) {
        let family_of = Owned(unsafe { CTFontDescriptorCopyAttribute(d, key) });
        // A family nobody has matches a fallback: only exact families count.
        if rust_string(family_of.0).as_deref() != Some(family) {
            continue;
        }
        let style = Owned(unsafe { CTFontDescriptorCopyAttribute(d, kCTFontStyleNameAttribute) });
        if let Some(s) = rust_string(style.0)
            && !out.contains(&s)
        {
            out.push(s);
        }
    }
    out.sort_by_key(|s| weight_order(s));
    out
}

/// Thin … Black, then the italics in the same order.
fn weight_order(style: &str) -> (bool, usize, String) {
    const WEIGHTS: [&str; 9] = [
        "thin",
        "extralight",
        "light",
        "regular",
        "medium",
        "semibold",
        "bold",
        "extrabold",
        "black",
    ];
    let s = style.to_lowercase().replace([' ', '-'], "");
    let italic = s.contains("italic") || s.contains("oblique");
    let base = s.replace("italic", "").replace("oblique", "");
    let base = match base.as_str() {
        "" | "book" | "normal" => "regular",
        "ultralight" => "extralight",
        "demibold" => "semibold",
        "ultrabold" => "extrabold",
        "heavy" => "black",
        b => b,
    };
    let rank = WEIGHTS.iter().position(|w| *w == base).unwrap_or(4);
    (italic, rank, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_sort_by_weight() {
        let mut s = vec![
            "Bold Italic",
            "ExtraBold",
            "Regular",
            "Thin",
            "Italic",
            "Medium",
        ];
        s.sort_by_key(|x| weight_order(x));
        assert_eq!(
            s,
            [
                "Thin",
                "Regular",
                "Medium",
                "ExtraBold",
                "Italic",
                "Bold Italic"
            ]
        );
    }

    #[test]
    fn asks_macos() {
        // Helvetica ships with every Mac.
        assert!(families().iter().any(|f| f == "Helvetica"));
        let found = styles("Helvetica");
        assert!(found.contains(&"Bold".to_string()), "{found:?}");
        assert!(styles("No Such Font Family 123").is_empty());
    }
}
