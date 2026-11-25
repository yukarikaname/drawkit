use super::*;

/// Whether the document has no paint on any layer.
#[unsafe(no_mangle)]
pub extern "C" fn dk_is_clear(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.document.is_clear()) as i32
}

/// Replace the document with a `.kra` or `.psd`. Returns 0 when the file cannot be read or holds
/// no raster layers.
///
/// # Safety
///
/// `path` must be a NUL-terminated C string, and `handle` must be a live editor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_import_kra(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    match drawkit_formats::read_kra(std::path::Path::new(path)) {
        Ok(document) => editor.import_document(&document) as i32,
        Err(error) => {
            eprintln!("import kra: {error}");
            0
        }
    }
}

/// Replace the document with a `.psd`. Returns 0 when the file cannot be read.
///
/// # Safety
///
/// `path` must be a NUL-terminated C string, and `handle` must be a live editor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_import_psd(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    match drawkit_formats::read_psd(std::path::Path::new(path)) {
        Ok(document) => editor.import_document(&document) as i32,
        Err(error) => {
            eprintln!("import psd: {error}");
            0
        }
    }
}

/// Write the document as a `.kra`.
///
/// # Safety
///
/// `path` must be a NUL-terminated C string, and `handle` must be a live editor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_export_kra(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    editor.export_kra(std::path::Path::new(path)) as i32
}

/// Write the document as a layered `.psd`.
///
/// # Safety
///
/// `path` must be a NUL-terminated C string, and `handle` must be a live editor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_export_psd(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    editor.export_psd(std::path::Path::new(path)) as i32
}

/// Write the rich stroke tape as a `.dkp` file. Returns 0 when nothing was recorded.
///
/// # Safety
///
/// `path` must be a NUL-terminated C string, and `handle` must be a live editor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_save_tape(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() || editor.stroke_tape().events.is_empty() {
        return 0;
    }
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return 0;
    };
    editor.save_tape(std::path::Path::new(path)) as i32
}

/// Open a project, Krita/Photoshop document or PNG/JPEG into a new editor sized to the file.
/// Returns null when the file cannot be read. The caller owns the returned handle.
///
/// # Safety
///
/// `path` must be a NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_open_file(path: *const c_char) -> *mut c_void {
    if path.is_null() {
        return std::ptr::null_mut();
    }
    let path = unsafe { CStr::from_ptr(path) };
    let Ok(path) = path.to_str() else {
        return std::ptr::null_mut();
    };
    match Editor::open_file(std::path::Path::new(path)) {
        Some(editor) => Box::into_raw(Box::new(editor)) as *mut c_void,
        None => std::ptr::null_mut(),
    }
}

/// Export every frame as a PNG sequence plus `animation.json` into a directory.
/// # Safety
///
/// `path` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_export_animation(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    let Ok(path) = unsafe { CStr::from_ptr(path) }.to_str() else {
        return 0;
    };
    editor.export_animation(std::path::Path::new(path)) as i32
}

/// Replace the editor with a PNG-sequence animation directory.
/// # Safety
///
/// `path` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_import_animation(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    let Ok(path) = unsafe { CStr::from_ptr(path) }.to_str() else {
        return 0;
    };
    let Some(imported) = Editor::open_animation(std::path::Path::new(path)) else {
        return 0;
    };
    *editor = imported;
    1
}

/// Export every frame as an Aseprite (.aseprite) file.
/// # Safety
///
/// `path` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_export_aseprite(handle: *mut c_void, path: *const c_char) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    let Ok(path) = unsafe { CStr::from_ptr(path) }.to_str() else {
        return 0;
    };
    editor.export_aseprite(std::path::Path::new(path)) as i32
}
