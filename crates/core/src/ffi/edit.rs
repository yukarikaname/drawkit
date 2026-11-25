use super::*;

#[unsafe(no_mangle)]
pub extern "C" fn dk_editor_new(width: u32, height: u32) -> *mut c_void {
    if width == 0 || height == 0 {
        return std::ptr::null_mut();
    }
    let editor = Editor::new(width as usize, height as usize, 256 << 20);
    Box::into_raw(Box::new(editor)) as *mut c_void
}

/// A shallow-in-time copy of the editor for a background save. The shell hands it to a worker
/// thread, writes the project there, and frees it; the live editor keeps taking strokes.
#[unsafe(no_mangle)]
pub extern "C" fn dk_editor_clone(handle: *mut c_void) -> *mut c_void {
    let Some(editor) = editor(handle) else {
        return std::ptr::null_mut();
    };
    editor.cancel_prediction();
    Box::into_raw(Box::new(editor.save_snapshot())) as *mut c_void
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_editor_free(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    // SAFETY: owned by this call from here on, and never handed out again.
    unsafe { drop(Box::from_raw(handle as *mut Editor)) };
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_color_space(handle: *mut c_void) -> i32 {
    editor(handle).map_or(0, |editor| editor.document.color_space.as_i32())
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_color_space(handle: *mut c_void, space: i32) {
    if let Some(editor) = editor(handle) {
        editor.document.color_space = crate::document::ColorSpace::from_i32(space);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_width(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.document.width() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_height(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.document.height() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_brush(handle: *mut c_void, kind: i32) {
    if let Some(editor) = editor(handle) {
        editor.set_brush(crate::brush::BrushKind::from_i32(kind));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_brush(handle: *mut c_void) -> i32 {
    editor(handle).map_or(0, |editor| editor.brush.kind.as_i32())
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_mirror(handle: *mut c_void, mode: i32) {
    if let Some(editor) = editor(handle) {
        editor.set_mirror(crate::tools::Mirror::from_i32(mode));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_mirror(handle: *mut c_void) -> i32 {
    editor(handle).map_or(0, |editor| editor.mirror.as_i32())
}

/// Set the symmetry axis in degrees. `both` adds the perpendicular axis.
#[unsafe(no_mangle)]
pub extern "C" fn dk_set_mirror_angle(handle: *mut c_void, degrees: f32, both: i32) {
    if let Some(editor) = editor(handle) {
        editor.set_mirror(crate::tools::Mirror {
            enabled: true,
            angle: degrees.to_radians().rem_euclid(std::f32::consts::PI),
            both: both != 0,
        });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_mirror_angle(handle: *mut c_void) -> f32 {
    editor(handle).map_or(0.0, |editor| editor.mirror.angle.to_degrees())
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_mirror_both(handle: *mut c_void) -> i32 {
    editor(handle).map_or(0, |editor| i32::from(editor.mirror.both))
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_stabilization(handle: *mut c_void, strength: f32) {
    if let Some(editor) = editor(handle) {
        editor.set_stabilization(strength);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_stroke_begin(handle: *mut c_void, time_ms: u64) {
    if let Some(editor) = editor(handle) {
        editor.begin_stroke(time_ms);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_stroke_end(handle: *mut c_void) {
    if let Some(editor) = editor(handle) {
        editor.end_stroke();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_stroke(
    handle: *mut c_void,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    pressure: f32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    time_ms: u64,
) {
    if let Some(editor) = editor(handle) {
        editor.stroke_at(
            (x0, y0),
            (x1, y1),
            radius,
            pressure,
            [red, green, blue, alpha],
            time_ms,
        );
    }
}

/// Paint a segment with tablet dynamics: pressure, tilt, azimuth and roll. The current brush uses
/// pressure; the other values are stored on the tape for a brush engine that uses them.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_stroke_ex(
    handle: *mut c_void,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    pressure: f32,
    tilt_x: f32,
    tilt_y: f32,
    azimuth: f32,
    roll: f32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    time_ms: u64,
) {
    if let Some(editor) = editor(handle) {
        editor.stroke_sample(
            (x0, y0),
            (x1, y1),
            radius,
            crate::tools::Dynamics {
                pressure,
                tilt: [tilt_x, tilt_y],
                azimuth,
                roll,
            },
            [red, green, blue, alpha],
            time_ms,
        );
    }
}

/// Erase a segment with tablet dynamics.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_erase_ex(
    handle: *mut c_void,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    pressure: f32,
    tilt_x: f32,
    tilt_y: f32,
    azimuth: f32,
    roll: f32,
    time_ms: u64,
) {
    if let Some(editor) = editor(handle) {
        editor.erase_sample(
            (x0, y0),
            (x1, y1),
            radius,
            crate::tools::Dynamics {
                pressure,
                tilt: [tilt_x, tilt_y],
                azimuth,
                roll,
            },
            time_ms,
        );
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_erase(
    handle: *mut c_void,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    pressure: f32,
    time_ms: u64,
) {
    if let Some(editor) = editor(handle) {
        editor.erase_at((x0, y0), (x1, y1), radius, pressure, time_ms);
    }
}

/// Fill with gap closing, expansion and feathering. The older `dk_fill` is the default policy.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_fill_ex(
    handle: *mut c_void,
    x: u32,
    y: u32,
    tolerance: f32,
    close: u32,
    expand: u32,
    feather: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    time_ms: u64,
) {
    if let Some(editor) = editor(handle) {
        editor.fill_with(
            x as usize,
            y as usize,
            tolerance,
            [red, green, blue, alpha],
            crate::fill::FillOptions {
                close: close as usize,
                expand: expand as usize,
                feather: feather as usize,
            },
            time_ms,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_remove_paper(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.remove_paper_from_layer()) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_prediction(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.cancel_prediction();
        editor.prediction_enabled = enabled != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_prediction(handle: *mut c_void) -> i32 {
    editor(handle).map_or(0, |editor| i32::from(editor.prediction_enabled))
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_stroke_dedupe(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.stroke_dedupe = enabled != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_stroke_dedupe(handle: *mut c_void) -> i32 {
    editor(handle).map_or(0, |editor| i32::from(editor.stroke_dedupe))
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_set_fill_snap(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.fill_snap = enabled != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_fill_snap(handle: *mut c_void) -> i32 {
    editor(handle).map_or(0, |editor| i32::from(editor.fill_snap))
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_fill(
    handle: *mut c_void,
    x: u32,
    y: u32,
    tolerance: f32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    time_ms: u64,
) {
    if let Some(editor) = editor(handle) {
        editor.fill_at(
            x as usize,
            y as usize,
            tolerance,
            [red, green, blue, alpha],
            time_ms,
        );
    }
}

/// Begin a progressive fill. The shell animates `dk_fill_step` and then calls `dk_fill_finish`;
/// the whole fill is one undo step and one tape event.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_fill_begin(
    handle: *mut c_void,
    x: u32,
    y: u32,
    tolerance: f32,
    close: u32,
    expand: u32,
    feather: u32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
    time_ms: u64,
) -> i32 {
    editor(handle).is_some_and(|editor| {
        editor
            .begin_fill_animation(
                x as usize,
                y as usize,
                tolerance,
                [red, green, blue, alpha],
                crate::fill::FillOptions {
                    close: close as usize,
                    expand: expand as usize,
                    feather: feather as usize,
                },
                time_ms,
            )
            .is_some()
    }) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_fill_step(handle: *mut c_void, progress: f32) -> i32 {
    editor(handle).is_some_and(|editor| editor.fill_animation_step(progress).is_some()) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_fill_finish(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.finish_fill_animation()) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_fill_cancel(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.cancel_fill_animation()) as i32
}

/// Whether there is a step to undo or redo; the shell uses it to dim its buttons.
#[unsafe(no_mangle)]
pub extern "C" fn dk_can_undo(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.can_undo()) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_can_redo(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.can_redo()) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_undo(handle: *mut c_void) -> i32 {
    editor(handle).and_then(|editor| editor.undo()).is_some() as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_redo(handle: *mut c_void) -> i32 {
    editor(handle).and_then(|editor| editor.redo()).is_some() as i32
}

/// Write the document as premultiplied RGBA8, `width * height * 4` bytes, into `out`.
///
/// Premultiplied because that is what CoreGraphics and the GPU both want; converting here keeps the
/// shells from each getting it slightly wrong.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to `width * height * 4` writable bytes for the document size.
#[unsafe(no_mangle)]
pub extern "C" fn dk_smudge_ex(
    handle: *mut c_void,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
    pressure: f32,
    strength: f32,
    time_ms: u64,
) {
    if let Some(editor) = editor(handle) {
        editor.smudge_at((x0, y0), (x1, y1), radius, pressure, strength, time_ms);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_clear(handle: *mut c_void, red: f32, green: f32, blue: f32, alpha: f32) {
    if let Some(editor) = editor(handle) {
        editor.clear([red, green, blue, alpha]);
    }
}

/// Turn the tiny stroke-shape model on or off. It only replaces a finished stroke; an erase or
/// fill group is never rewritten.
#[unsafe(no_mangle)]
pub extern "C" fn dk_set_auto_shape(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.auto_shape = enabled != 0;
    }
}

/// Allow a held stroke to snap even when Auto Shape is off. The shell calls `dk_auto_shape` only
/// when it saw the pen rest at the end of the stroke.
#[unsafe(no_mangle)]
pub extern "C" fn dk_set_quick_shape(handle: *mut c_void, enabled: i32) {
    if let Some(editor) = editor(handle) {
        editor.quick_shape = enabled != 0;
    }
}

/// Replace the last stroke with the ideal line, ellipse or rectangle when the model is confident.
#[unsafe(no_mangle)]
pub extern "C" fn dk_auto_shape(handle: *mut c_void) -> i32 {
    editor(handle).is_some_and(|editor| editor.auto_shape()) as i32
}
