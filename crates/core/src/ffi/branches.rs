//! The experimental history branches a shell can list and switch.

use std::os::raw::c_void;

use super::editor;

#[unsafe(no_mangle)]
pub extern "C" fn dk_history_branches_enabled(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.history_branches_enabled()) as i32
}

/// Turn the feature on or off. Disabling it forgets every dormant branch.
#[unsafe(no_mangle)]
pub extern "C" fn dk_set_history_branches(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.set_history_branches(enabled != 0);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_history_branch_count(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.history_branch_count() as u32)
}

/// Write `id`, `parent` (`u64::MAX` for the root), `current` and `bytes` into `out`.
///
/// # Safety
/// `out` must point at four writable `u64` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_history_branch(handle: *mut c_void, index: u32, out: *mut u64) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let Some((id, parent, current, bytes)) = editor.history_branch(index as usize) else {
        return 0;
    };
    // SAFETY: the shell passed room for four u64 values.
    let values = unsafe { std::slice::from_raw_parts_mut(out, 4) };
    values[0] = id;
    values[1] = parent.unwrap_or(u64::MAX);
    values[2] = u64::from(current);
    values[3] = bytes as u64;
    1
}

/// The selected branch, or -1 when the live path has not been stored yet.
#[unsafe(no_mangle)]
pub extern "C" fn dk_history_branch_current(handle: *mut c_void) -> i64 {
    editor(handle).map_or(-1, |editor| {
        editor.current_history_branch().map_or(-1, |id| id as i64)
    })
}

/// Whether the undo sequence has an abandoned path that the next edit turns into a branch.
#[unsafe(no_mangle)]
pub extern "C" fn dk_history_branch_pending(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.pending_history_branch()) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_history_branch_switch(handle: *mut c_void, id: u64) -> i32 {
    editor(handle).is_some_and(|editor| editor.switch_history_branch(id)) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_history_branch_clear(handle: *mut c_void) {
    if let Some(editor) = editor(handle) {
        editor.clear_history_branches();
    }
}
