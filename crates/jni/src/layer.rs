use super::*;

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerCount(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(0, |editor| editor.document.layers.len() as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerActive(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(0, |editor| editor.document.active as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerSelect(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) {
    if let Some(editor) = editor(handle)
        && index >= 0
        && (index as usize) < editor.document.layers.len()
    {
        editor.document.active = index as usize;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerAdd(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(-1, |editor| {
        let name = format!("Layer {}", editor.document.layers.len() + 1);
        editor.document.add_layer(name) as jint
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerMove(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    steps: jint,
) -> jint {
    editor(handle).map_or(0, |editor| {
        let before = editor.document.active;
        let Some((after_index, applied)) = editor.document.move_layer(index as usize, steps) else {
            return 0;
        };
        let after = editor.document.active;
        editor.record_layer_move(index as usize, after_index, applied, before, after);
        1
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerRemove(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(-1, |editor| editor.document.remove_active() as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerVisible(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jboolean {
    editor(handle)
        .and_then(|editor| editor.document.layers.get(index.max(0) as usize))
        .map_or(0, |layer| layer.visible as jboolean)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerSetVisible(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    visible: jboolean,
) {
    if let Some(editor) = editor(handle)
        && let Some(layer) = editor.document.layers.get_mut(index.max(0) as usize)
    {
        layer.visible = visible != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerOpacity(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jfloat {
    editor(handle)
        .and_then(|editor| editor.document.layers.get(index.max(0) as usize))
        .map_or(1.0, |layer| layer.opacity)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerSetOpacity(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    opacity: jfloat,
) {
    if let Some(editor) = editor(handle)
        && let Some(layer) = editor.document.layers.get_mut(index.max(0) as usize)
    {
        layer.opacity = opacity.clamp(0.0, 1.0);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerName(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jni::sys::jstring {
    let name = editor(handle)
        .and_then(|editor| editor.document.layers.get(index.max(0) as usize))
        .map_or("Layer", |layer| layer.name.as_str());
    env.new_string(name)
        .map_or(std::ptr::null_mut(), |string| string.into_raw())
}

/// Copy `width * height` premultiplied ARGB thumbnail pixels into `out`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerThumbnail(
    env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    width: jint,
    height: jint,
    out: JIntArray,
) -> jint {
    let Some(editor) = editor(handle) else {
        return 0;
    };
    if width <= 0 || height <= 0 || width > 512 || height > 512 {
        return 0;
    }
    let Some(pixels) =
        editor.layer_thumbnail(index.max(0) as usize, width as usize, height as usize)
    else {
        return 0;
    };
    let colors: Vec<jint> = pixels.into_iter().map(argb_premultiplied).collect();
    env.set_int_array_region(&out, 0, &colors).is_ok() as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerAddGroup(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
) -> jint {
    editor(handle).map_or(-1, |editor| {
        let name = format!("Group {}", editor.document.layers.len() + 1);
        editor.document.add_group(name) as jint
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerAddFill(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    red: jfloat,
    green: jfloat,
    blue: jfloat,
    alpha: jfloat,
) -> jint {
    editor(handle).map_or(-1, |editor| {
        let name = format!("Fill {}", editor.document.layers.len() + 1);
        editor
            .document
            .add_fill_layer(name, [red, green, blue, alpha]) as jint
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerKind(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jint {
    editor(handle).map_or(0, |editor| {
        editor
            .document
            .layers
            .get(index as usize)
            .map_or(0, |layer| match layer.kind {
                drawkit_core::document::LayerKind::Paint => 0,
                drawkit_core::document::LayerKind::Group => 1,
                drawkit_core::document::LayerKind::Fill => 2,
            })
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerParent(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jint {
    editor(handle).map_or(-1, |editor| {
        editor
            .document
            .layers
            .get(index as usize)
            .and_then(|layer| layer.parent)
            .map_or(-1, |parent| parent as jint)
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerExpanded(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jboolean {
    editor(handle).is_some_and(|editor| {
        editor
            .document
            .layers
            .get(index as usize)
            .is_some_and(|layer| layer.expanded)
    }) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerSetExpanded(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    expanded: jboolean,
) {
    if let Some(editor) = editor(handle)
        && let Some(layer) = editor.document.layers.get_mut(index as usize)
    {
        layer.expanded = expanded != 0;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerReference(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jboolean {
    editor(handle)
        .and_then(|editor| editor.document.layers.get(index as usize))
        .is_some_and(|layer| layer.reference) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerSetReference(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    reference: jboolean,
) {
    if let Some(editor) = editor(handle)
        && let Some(layer) = editor.document.layers.get_mut(index as usize)
    {
        let before = layer.meta();
        layer.reference = reference != 0;
        editor.record_layer_state(
            index as usize,
            drawkit_core::tools::LayerField::Reference,
            before,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerTag(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
) -> jint {
    editor(handle)
        .and_then(|editor| editor.document.layers.get(index as usize))
        .map_or(0, |layer| layer.color_tag as jint)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerSetTag(
    _env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    tag: jint,
) {
    if let Some(editor) = editor(handle)
        && let Some(layer) = editor.document.layers.get_mut(index as usize)
    {
        let before = layer.meta();
        layer.color_tag = tag.clamp(0, 6) as u8;
        editor.record_layer_state(
            index as usize,
            drawkit_core::tools::LayerField::ColorTag,
            before,
        );
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_drawkit_android_core_Native_layerSetName(
    mut env: JNIEnv,
    _this: JObject,
    handle: jlong,
    index: jint,
    name: JString,
) {
    let Ok(name) = env.get_string(&name) else {
        return;
    };
    let name = name.to_string_lossy().to_string();
    if name.is_empty() {
        return;
    }
    if let Some(editor) = editor(handle)
        && let Some(layer) = editor.document.layers.get_mut(index as usize)
    {
        let before = layer.meta();
        layer.name = name;
        editor.record_layer_state(
            index as usize,
            drawkit_core::tools::LayerField::Name,
            before,
        );
    }
}
