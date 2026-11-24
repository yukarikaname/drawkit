//! Palette files are shell state, not editor state: the shell asks for the colours and decides
//! where to put them. GIMP `.gpl` and Adobe `.ase` are the interchange pair.

use std::ffi::CStr;
use std::os::raw::c_char;

use drawkit_formats::{PaletteEntry, PaletteFormat, read_palette, write_palette};

/// Read a GIMP or Adobe palette into packed sRGB triples. Returns the number of colours written,
/// capped at `max_colors`.
/// # Safety
///
/// `path` must be a NUL-terminated string; `out` must point to `max_colors * 3` writable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_read_palette(
    path: *const c_char,
    out: *mut f32,
    max_colors: u32,
) -> i32 {
    if path.is_null() || out.is_null() || max_colors == 0 {
        return 0;
    }
    // SAFETY: the shell passed a NUL-terminated string and the output buffer above.
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    let Ok(entries) = read_palette(std::path::Path::new(path)) else {
        return 0;
    };
    let count = entries.len().min(max_colors as usize);
    // SAFETY: the shell allocated `max_colors * 3` floats.
    let out = unsafe { std::slice::from_raw_parts_mut(out, max_colors as usize * 3) };
    for (index, entry) in entries.iter().take(count).enumerate() {
        out[index * 3..index * 3 + 3].copy_from_slice(&entry.color);
    }
    count as i32
}

/// Write sRGB triples as a palette. `format` is 0 for GIMP `.gpl` and 1 for Adobe `.ase`.
/// # Safety
///
/// `path` must be NUL-terminated; `colors` must point to `count * 3` readable floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_write_palette(
    path: *const c_char,
    colors: *const f32,
    count: u32,
    format: i32,
) -> i32 {
    if path.is_null() || colors.is_null() || count == 0 {
        return 0;
    }
    // SAFETY: the shell passed the two buffers named in the contract.
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    // SAFETY: count triples were passed by the shell.
    let colors = unsafe { std::slice::from_raw_parts(colors, count as usize * 3) };
    let entries: Vec<PaletteEntry> = colors
        .as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .map(|(index, color)| PaletteEntry::new(format!("Colour {}", index + 1), *color))
        .collect();
    let format = if format == 1 {
        PaletteFormat::Ase
    } else {
        PaletteFormat::Gpl
    };
    i32::from(write_palette(std::path::Path::new(path), &entries, format).is_ok())
}
