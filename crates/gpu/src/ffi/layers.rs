//! Incremental resident-layer entry points. A structural edit keeps the other layers' textures
//! and only rearranges the stack, so the shell does not rebuild every layer.

use std::os::raw::c_void;

use super::canvas::canvas;

/// Number of layers the resident stack holds.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_layer_count(handle: *mut c_void) -> u32 {
    canvas(handle).map_or(0, |surface| surface.layer_count() as u32)
}

/// Insert an empty resident layer at `index` on a surface whose canvas is `width * height`.
///
/// Returns 1 on success; 0 when the handle is null, `index` is past the end, the size does not
/// match the canvas, or the stack cannot take the layer. Group and clip references above the
/// insertion move up with their layers. The tile undo journal is dropped, because its steps name
/// layer positions; the shell rebuilds its GPU history, as after `reset_layers`.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_insert_layer(
    handle: *mut c_void,
    index: u32,
    width: u32,
    height: u32,
) -> i32 {
    canvas(handle).is_some_and(|surface| surface.insert_layer(index as usize, width, height)) as i32
}

/// Remove layer `index` and shift the layers above it down.
///
/// Returns 1 when the layer was removed. Returns 0, with the stack untouched, when the handle is
/// null, the index does not exist, or another layer groups under or clips through it; ungroup or
/// unclip that layer first.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_remove_layer(handle: *mut c_void, index: u32) -> i32 {
    canvas(handle).is_some_and(|surface| surface.remove_layer(index as usize)) as i32
}

/// Move layer `from` to `to`. Only the draw order changes; pixels, groups and clips follow their
/// layer.
///
/// Returns 1 on success; 0 when the handle is null or either index does not exist.
#[unsafe(no_mangle)]
pub extern "C" fn dk_canvas_surface_move_layer(handle: *mut c_void, from: u32, to: u32) -> i32 {
    canvas(handle).is_some_and(|surface| surface.move_layer(from as usize, to as usize)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_null_handle_has_no_layers() {
        assert_eq!(dk_canvas_surface_layer_count(std::ptr::null_mut()), 0);
    }

    #[test]
    fn a_null_handle_refuses_every_edit() {
        assert_eq!(
            dk_canvas_surface_insert_layer(std::ptr::null_mut(), 0, 1, 1),
            0
        );
        assert_eq!(dk_canvas_surface_remove_layer(std::ptr::null_mut(), 0), 0);
        assert_eq!(dk_canvas_surface_move_layer(std::ptr::null_mut(), 0, 1), 0);
    }
}
