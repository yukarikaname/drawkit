use super::*;

pub(super) fn layer_at<'a>(
    handle: *mut c_void,
    index: u32,
) -> Option<&'a mut crate::document::Layer> {
    editor(handle)?.document.layers.get_mut(index as usize)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_count(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.document.layers.len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_active(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| editor.document.active as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_select(handle: *mut c_void, index: u32) {
    if let Some(editor) = editor(handle)
        && (index as usize) < editor.document.layers.len()
    {
        editor.document.active = index as usize;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_kind(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).map_or(0, |editor| {
        match editor
            .document
            .layers
            .get(index as usize)
            .map(|layer| layer.kind)
        {
            Some(crate::document::LayerKind::Group) => 1,
            _ => 0,
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_parent(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).map_or(-1, |editor| {
        editor
            .document
            .layers
            .get(index as usize)
            .and_then(|layer| layer.parent)
            .map_or(-1, |parent| parent as i32)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_add_fill(
    handle: *mut c_void,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
) -> u32 {
    editor(handle).map_or(0, |editor| {
        let before = editor.document.active;
        let name = format!("Fill {}", editor.document.layers.len() + 1);
        let index = editor
            .document
            .add_fill_layer(name, [red, green, blue, alpha]);
        let layer = editor.document.layers[index].clone();
        editor.record_layer_insertion(index, layer, before, editor.document.active);
        index as u32
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_add_group(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| {
        let before = editor.document.active;
        let name = format!("Group {}", editor.document.layers.len() + 1);
        let index = editor.document.add_group(name);
        let layer = editor.document.layers[index].clone();
        editor.record_layer_insertion(index, layer, before, editor.document.active);
        index as u32
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_clipping(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).map_or(0, |editor| {
        editor
            .document
            .layers
            .get(index as usize)
            .is_some_and(|layer| layer.clipping) as i32
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_clipping(handle: *mut c_void, index: u32, clipping: i32) {
    edit_layer(handle, index, LayerField::Clipping, |layer| {
        layer.clipping = clipping != 0;
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_alpha_locked(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).map_or(0, |editor| {
        editor
            .document
            .layers
            .get(index as usize)
            .is_some_and(|layer| layer.alpha_locked) as i32
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_alpha_locked(handle: *mut c_void, index: u32, locked: i32) {
    edit_layer(handle, index, LayerField::AlphaLocked, |layer| {
        layer.alpha_locked = locked != 0;
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_tag(handle: *mut c_void, index: u32) -> i32 {
    layer_at(handle, index).map_or(0, |layer| layer.color_tag as i32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_tag(handle: *mut c_void, index: u32, tag: i32) {
    edit_layer(handle, index, LayerField::ColorTag, |layer| {
        layer.color_tag = tag.clamp(0, 6) as u8;
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_locked(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).map_or(0, |editor| {
        editor
            .document
            .layers
            .get(index as usize)
            .is_some_and(|layer| layer.locked) as i32
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_locked(handle: *mut c_void, index: u32, locked: i32) {
    edit_layer(handle, index, LayerField::Locked, |layer| {
        layer.locked = locked != 0;
    });
}

/// Whether a layer is visible through every ancestor.
#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_effective_visible(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).map_or(0, |editor| {
        i32::from(editor.document.effective_visible(index as usize))
    })
}

/// A layer's own opacity times every ancestor's.
#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_effective_opacity(handle: *mut c_void, index: u32) -> f32 {
    editor(handle).map_or(1.0, |editor| {
        editor.document.effective_opacity(index as usize)
    })
}

/// The solid colour of a fill layer.
///
/// # Safety
/// `out` must point at four `f32` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_layer_fill(handle: *mut c_void, index: u32, out: *mut f32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let Some(layer) = editor.document.layers.get(index as usize) else {
        return 0;
    };
    // SAFETY: the shell passed four writable floats.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, 4) };
    buffer.copy_from_slice(&layer.fill);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_blend(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).map_or(0, |editor| {
        editor
            .document
            .layers
            .get(index as usize)
            .map_or(0, |layer| layer.blend.as_i32())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_blend(handle: *mut c_void, index: u32, blend: i32) {
    edit_layer(handle, index, LayerField::Blend, |layer| {
        layer.blend = crate::document::BlendMode::from_i32(blend);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_expanded(handle: *mut c_void, index: u32) -> i32 {
    editor(handle).map_or(1, |editor| {
        editor
            .document
            .layers
            .get(index as usize)
            .map_or(1, |layer| i32::from(layer.expanded))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_expanded(handle: *mut c_void, index: u32, expanded: i32) {
    // Disclosure is view state, not a document edit, so it stays out of the undo timeline.
    if let Some(layer) = layer_at(handle, index) {
        layer.expanded = expanded != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_add(handle: *mut c_void) -> u32 {
    editor(handle).map_or(0, |editor| {
        let before = editor.document.active;
        let name = format!("Layer {}", editor.document.layers.len() + 1);
        let index = editor.document.add_layer(name);
        let layer = editor.document.layers[index].clone();
        editor.record_layer_insertion(index, layer, before, editor.document.active);
        index as u32
    })
}

/// True when a layer has no ink, so a rebuild can skip its upload.
#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_is_clear(handle: *mut c_void, index: u32) -> i32 {
    editor(handle)
        .and_then(|editor| editor.document.layers.get(index as usize))
        .is_some_and(|layer| layer.surface.is_clear()) as i32
}

/// Move a layer or group by `steps` siblings, positive towards the front. The whole subtree moves
/// with it; the shell redraws the palette and the composite.
#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_move(handle: *mut c_void, index: u32, steps: i32) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let before = editor.document.active;
    let Some((after_index, applied)) = editor.document.move_layer(index as usize, steps) else {
        return 0;
    };
    let after = editor.document.active;
    editor.record_layer_move(index as usize, after_index, applied, before, after);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_remove(handle: *mut c_void) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(removal) = editor.document.remove_active_collect() else {
        return 0;
    };
    editor.record_layer_removal(
        removal.removed,
        removal.inserted,
        removal.active_before,
        removal.active_after,
    );
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_visible(handle: *mut c_void, index: u32) -> i32 {
    layer_at(handle, index).map_or(0, |layer| layer.visible as i32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_visible(handle: *mut c_void, index: u32, visible: i32) {
    edit_layer(handle, index, LayerField::Visible, |layer| {
        layer.visible = visible != 0;
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_opacity(handle: *mut c_void, index: u32) -> f32 {
    layer_at(handle, index).map_or(0.0, |layer| layer.opacity)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_opacity(handle: *mut c_void, index: u32, opacity: f32) {
    edit_layer(handle, index, LayerField::Opacity, |layer| {
        layer.opacity = opacity.clamp(0.0, 1.0);
    });
}

/// Add a layer above the active one from premultiplied RGBA8, for baking a reference.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `pixels` must point to `width * height * 4` readable bytes that match the document size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_layer_add_image(
    handle: *mut c_void,
    width: u32,
    height: u32,
    pixels: *const u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if pixels.is_null() || width == 0 || height == 0 {
        return 0;
    }
    if width as usize != editor.document.width() || height as usize != editor.document.height() {
        return 0;
    }
    // SAFETY: the shell passed `width * height * 4` bytes, which is the whole contract.
    let source =
        unsafe { std::slice::from_raw_parts(pixels, width as usize * height as usize * 4) };
    let before = editor.document.active;
    let added = editor.add_image_layer(source, width as usize, height as usize);
    if added {
        let index = editor.document.active;
        let layer = editor.document.layers[index].clone();
        editor.record_layer_insertion(index, layer, before, editor.document.active);
    }
    added as i32
}

/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `path` must be a NUL-terminated C string.
/// Downscale one layer into premultiplied RGBA8, for the layer palette.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to `width * height * 4` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_layer_thumbnail(
    handle: *mut c_void,
    index: u32,
    width: u32,
    height: u32,
    out: *mut u8,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 || width > 512 || height > 512 {
        return 0;
    }
    let Some(pixels) = editor.layer_thumbnail(index as usize, width as usize, height as usize)
    else {
        return 0;
    };
    // SAFETY: the shell allocated `width * height * 4` bytes.
    let buffer =
        unsafe { std::slice::from_raw_parts_mut(out, width as usize * height as usize * 4) };
    for (index, pixel) in pixels.iter().enumerate() {
        let alpha = pixel[3].clamp(0.0, 1.0);
        buffer[index * 4] = (pixel[0].clamp(0.0, 1.0) * 255.0).round() as u8;
        buffer[index * 4 + 1] = (pixel[1].clamp(0.0, 1.0) * 255.0).round() as u8;
        buffer[index * 4 + 2] = (pixel[2].clamp(0.0, 1.0) * 255.0).round() as u8;
        buffer[index * 4 + 3] = (alpha * 255.0).round() as u8;
    }
    1
}

/// Copy a layer's UTF-8 name into `out`, NUL-terminated, and return the bytes written.
/// # Safety
///
/// `handle` must be a live editor from `dk_editor_new`, and `out` must point to `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_layer_name(
    handle: *mut c_void,
    index: u32,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    let Some(layer) = editor.document.layers.get(index as usize) else {
        return 0;
    };
    if out.is_null() || capacity == 0 {
        return 0;
    }
    let bytes = layer.name.as_bytes();
    let length = bytes.len().min(capacity as usize - 1);
    // SAFETY: the shell passed `capacity` bytes for the name.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, capacity as usize) };
    buffer[..length].copy_from_slice(&bytes[..length]);
    buffer[length] = 0;
    length as i32
}

/// Rename a layer. The change is one undo step, recorded when the shell commits the edit.
/// # Safety
///
/// `name` must be a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dk_layer_set_name(
    handle: *mut c_void,
    index: u32,
    name: *const c_char,
) -> i32 {
    if name.is_null() {
        return 0;
    }
    // SAFETY: the shell passed a NUL-terminated string.
    let Ok(name) = unsafe { CStr::from_ptr(name) }.to_str() else {
        return 0;
    };
    if name.is_empty() || editor(handle).is_none() {
        return 0;
    }
    edit_layer(handle, index, LayerField::Name, |layer| {
        layer.name = name.to_owned();
    });
    1
}
