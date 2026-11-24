//! UI strings, shared by every shell through `dk_text`.
//!
//! The table lives in `shared/strings.json` and is compiled into the core, so a platform does not
//! carry its own copy and a missing translation fails the test here rather than showing later.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::OnceLock;

const SOURCE: &str = include_str!("../../../shared/strings.json");

struct Table {
    fallback: String,
    strings: HashMap<String, HashMap<String, CString>>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(build)
}

fn build() -> Table {
    let value: serde_json::Value =
        serde_json::from_str(SOURCE).expect("shared/strings.json is valid JSON");
    let fallback = value
        .get("fallback")
        .and_then(|value| value.as_str())
        .unwrap_or("en")
        .to_string();
    let mut strings = HashMap::new();
    if let Some(rows) = value.get("strings").and_then(|value| value.as_object()) {
        for (key, row) in rows {
            let mut languages = HashMap::new();
            if let Some(row) = row.as_object() {
                for (language, text) in row {
                    let text = text.as_str().unwrap_or_default();
                    languages.insert(language.clone(), CString::new(text).unwrap_or_default());
                }
            }
            strings.insert(key.clone(), languages);
        }
    }
    Table { fallback, strings }
}

/// The string for a key. A missing language uses the fallback, a missing key is returned as is so
/// the caller sees what broke instead of an empty label.
pub fn text(key: &str, language: &str) -> String {
    let table = table();
    let Some(row) = table.strings.get(key) else {
        return key.to_string();
    };
    row.get(language)
        .or_else(|| row.get(&table.fallback))
        .map(|text| text.to_string_lossy().into_owned())
        .unwrap_or_else(|| key.to_string())
}

/// Write `text(key, language)` into `out`, NUL-terminated, and return its byte length. With a short
/// buffer the output is truncated; the returned length is always what the whole string needs.
///
/// # Safety
/// `key` and `language` must be NUL-terminated C strings. `out` must point at `capacity` writable
/// bytes, or be null with `capacity` zero to ask for the length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_text(
    key: *const c_char,
    language: *const c_char,
    out: *mut c_char,
    capacity: usize,
) -> i32 {
    if key.is_null() {
        return 0;
    }
    // SAFETY: the caller passed NUL-terminated strings.
    let key = unsafe { CStr::from_ptr(key) }.to_string_lossy();
    let language = if language.is_null() {
        ""
    } else {
        // SAFETY: as above.
        unsafe { CStr::from_ptr(language) }.to_str().unwrap_or("")
    };
    let text = text(&key, language);
    let bytes = text.as_bytes();
    if !out.is_null() && capacity > 0 {
        let count = bytes.len().min(capacity - 1);
        // SAFETY: the caller granted `capacity` writable bytes.
        let target = unsafe { std::slice::from_raw_parts_mut(out as *mut u8, capacity) };
        target[..count].copy_from_slice(&bytes[..count]);
        target[count] = 0;
    }
    bytes.len() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_exists_in_every_language() {
        let table = table();
        assert!(!table.strings.is_empty());
        for (key, row) in &table.strings {
            for language in ["en", "zh-Hans", "ja"] {
                let value = row
                    .get(language)
                    .unwrap_or_else(|| panic!("{key} has no {language}"));
                assert!(!value.to_string_lossy().is_empty(), "{key}/{language}");
            }
        }
    }

    #[test]
    fn language_and_key_fallbacks_are_visible() {
        assert_eq!(text("welcomeStart", "zh-Hans"), "开始绘画");
        assert_eq!(text("welcomeStart", "fr"), text("welcomeStart", "en"));
        assert_eq!(text("no.such.key", "en"), "no.such.key");
    }

    #[test]
    fn the_c_api_reports_the_length_and_truncates() {
        let key = CString::new("welcomeStart").unwrap();
        let language = CString::new("ja").unwrap();
        let needed = unsafe { dk_text(key.as_ptr(), language.as_ptr(), std::ptr::null_mut(), 0) };
        assert!(needed > 0);
        let mut buffer = vec![0_i8; needed as usize + 1];
        let written = unsafe {
            dk_text(
                key.as_ptr(),
                language.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
            )
        };
        assert_eq!(written, needed);
        let value = unsafe { CStr::from_ptr(buffer.as_ptr()) };
        assert_eq!(value.to_str().unwrap(), "描き始める");
    }
}
