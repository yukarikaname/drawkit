use super::layer::layer_at;
use super::*;

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_reference(handle: *mut c_void, index: u32) -> i32 {
    layer_at(handle, index).map_or(0, |layer| layer.reference as i32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_layer_set_reference(handle: *mut c_void, index: u32, reference: i32) {
    edit_layer(handle, index, LayerField::Reference, |layer| {
        layer.reference = reference != 0;
    });
}
