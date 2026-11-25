use std::os::raw::c_void;

use drawkit_figure::{Camera, Figure, Joint, Light, Lighting, PosePreset, Preset};
use glam::Vec3;

use crate::Renderer;

struct GpuFigure {
    figure: Figure,
    renderer: Renderer,
    lighting: Lighting,
}

fn scene<'a>(handle: *mut c_void) -> Option<&'a mut GpuFigure> {
    if handle.is_null() {
        return None;
    }
    // SAFETY: the pointer came from `dk_figure_new` and the shell frees it exactly once.
    Some(unsafe { &mut *(handle as *mut GpuFigure) })
}

fn preset_of(value: u32) -> Preset {
    match value {
        1 => Preset::RealisticMale,
        2 => Preset::RealisticFemale,
        3 => Preset::Anime,
        4 => Preset::Chibi,
        _ => Preset::RealisticNeutral,
    }
}

fn pose_of(value: u32) -> PosePreset {
    match value {
        1 => PosePreset::TPose,
        2 => PosePreset::ArmsUp,
        3 => PosePreset::Contrapposto,
        _ => PosePreset::APose,
    }
}

fn joint_of(value: u32) -> Option<Joint> {
    Joint::ALL.get(value as usize).copied()
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_available() -> i32 {
    i32::from(Renderer::new().is_some())
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_new(preset: u32) -> *mut c_void {
    let Some(renderer) = Renderer::new() else {
        return std::ptr::null_mut();
    };
    let figure = Figure::from_preset(preset_of(preset));
    Box::into_raw(Box::new(GpuFigure {
        figure,
        renderer,
        lighting: Lighting::default(),
    })) as *mut c_void
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_free(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    // SAFETY: owned by this call from here on, and never handed out again.
    unsafe { drop(Box::from_raw(handle as *mut GpuFigure)) };
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_height(handle: *mut c_void) -> f32 {
    scene(handle).map_or(0.0, |scene| scene.figure.proportions.height)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_set_preset(handle: *mut c_void, preset: u32) {
    if let Some(scene) = scene(handle) {
        scene.figure.set_preset(preset_of(preset));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_pose(handle: *mut c_void, preset: u32) {
    if let Some(scene) = scene(handle) {
        scene.figure.apply_pose_preset(pose_of(preset));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_reset(handle: *mut c_void) {
    if let Some(scene) = scene(handle) {
        scene.figure.reset_pose();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_mirror(handle: *mut c_void) {
    if let Some(scene) = scene(handle) {
        scene.figure.mirror();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_rotate(
    handle: *mut c_void,
    joint: u32,
    axis_x: f32,
    axis_y: f32,
    axis_z: f32,
    radians: f32,
) -> i32 {
    let Some(scene) = scene(handle) else {
        return 0;
    };
    let Some(joint) = joint_of(joint) else {
        return 0;
    };
    let axis = Vec3::new(axis_x, axis_y, axis_z);
    if axis.length_squared() < 1e-8 || !radians.is_finite() {
        return 0;
    }
    scene.figure.rotate(joint, axis, radians);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_aim(
    handle: *mut c_void,
    joint: u32,
    target_x: f32,
    target_y: f32,
    target_z: f32,
) -> i32 {
    let Some(scene) = scene(handle) else {
        return 0;
    };
    let Some(joint) = joint_of(joint) else {
        return 0;
    };
    scene
        .figure
        .aim(joint, Vec3::new(target_x, target_y, target_z)) as i32
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_figure_ik(
    handle: *mut c_void,
    upper: u32,
    lower: u32,
    end: u32,
    target_x: f32,
    target_y: f32,
    target_z: f32,
    pole_x: f32,
    pole_y: f32,
    pole_z: f32,
) -> i32 {
    let Some(scene) = scene(handle) else {
        return 0;
    };
    let (Some(upper), Some(lower), Some(end)) = (joint_of(upper), joint_of(lower), joint_of(end))
    else {
        return 0;
    };
    let target = Vec3::new(target_x, target_y, target_z);
    let pole = Vec3::new(pole_x, pole_y, pole_z);
    scene.figure.solve_ik(upper, lower, end, target, pole) as i32
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_figure_pick(
    handle: *mut c_void,
    origin_x: f32,
    origin_y: f32,
    origin_z: f32,
    direction_x: f32,
    direction_y: f32,
    direction_z: f32,
    radius: f32,
) -> i32 {
    let Some(scene) = scene(handle) else {
        return -1;
    };
    let origin = Vec3::new(origin_x, origin_y, origin_z);
    let direction = Vec3::new(direction_x, direction_y, direction_z);
    scene
        .figure
        .pick(origin, direction, radius.max(0.001))
        .map_or(-1, |(joint, _)| joint.index() as i32)
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_joint(handle: *mut c_void, joint: u32, out: *mut f32) -> i32 {
    let Some(scene) = scene(handle) else {
        return 0;
    };
    let Some(joint) = joint_of(joint) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    let world = scene.figure.pose.world(&scene.figure.skeleton);
    let position = world[joint.index()].position;
    // SAFETY: the shell passed room for three floats.
    let buffer = unsafe { std::slice::from_raw_parts_mut(out, 3) };
    buffer.copy_from_slice(&[position.x, position.y, position.z]);
    1
}

/// Render into premultiplied RGBA8, `width * height * 4` bytes.
/// Set the three-light rig. Angles are radians; warmth is -1 (cool) to 1 (warm), zero neutral.
/// Returns 1 when the handle is live and every value is finite.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn dk_figure_set_lighting(
    handle: *mut c_void,
    key_azimuth: f32,
    key_elevation: f32,
    key_intensity: f32,
    key_warmth: f32,
    fill_azimuth: f32,
    fill_elevation: f32,
    fill_intensity: f32,
    fill_warmth: f32,
    rim_azimuth: f32,
    rim_elevation: f32,
    rim_intensity: f32,
    rim_warmth: f32,
    ambient: f32,
) -> i32 {
    let Some(scene) = scene(handle) else {
        return 0;
    };
    let values = [
        key_azimuth,
        key_elevation,
        key_intensity,
        key_warmth,
        fill_azimuth,
        fill_elevation,
        fill_intensity,
        fill_warmth,
        rim_azimuth,
        rim_elevation,
        rim_intensity,
        rim_warmth,
        ambient,
    ];
    if values.iter().any(|value| !value.is_finite()) {
        return 0;
    }
    scene.lighting = Lighting::new(
        Light::from_angles(
            key_azimuth,
            key_elevation.clamp(-1.5, 1.5),
            Light::tint(key_warmth),
            key_intensity,
        ),
        Light::from_angles(
            fill_azimuth,
            fill_elevation.clamp(-1.5, 1.5),
            Light::tint(fill_warmth),
            fill_intensity,
        ),
        Light::from_angles(
            rim_azimuth,
            rim_elevation.clamp(-1.5, 1.5),
            Light::tint(rim_warmth),
            rim_intensity,
        ),
        ambient,
    );
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn dk_figure_render(
    handle: *mut c_void,
    width: u32,
    height: u32,
    yaw: f32,
    pitch: f32,
    distance: f32,
    out: *mut u8,
) -> i32 {
    let Some(scene) = scene(handle) else {
        return 0;
    };
    if out.is_null() || width == 0 || height == 0 || width > 8192 || height > 8192 {
        return 0;
    }
    let target = Vec3::new(0.0, scene.figure.proportions.height * 0.55, 0.0);
    let camera = Camera::orbit(target, distance.max(0.2), yaw, pitch.clamp(-1.4, 1.4));
    let mesh = scene.figure.mesh();
    let pixels = scene.renderer.render_with_lighting(
        &mesh,
        &camera,
        width,
        height,
        [0.74, 0.68, 0.62],
        &scene.lighting,
    );
    if pixels.is_empty() {
        return 0;
    }
    // SAFETY: the shell allocated exactly `width * height * 4` bytes.
    let buffer =
        unsafe { std::slice::from_raw_parts_mut(out, width as usize * height as usize * 4) };
    buffer.copy_from_slice(&pixels);
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figure_renders_through_the_gpu_surface() {
        let handle = dk_figure_new(0);
        if handle.is_null() {
            return;
        }
        let mut buffer = vec![0_u8; 32 * 48 * 4];
        assert_eq!(
            dk_figure_render(handle, 32, 48, 0.2, 0.1, 4.5, buffer.as_mut_ptr()),
            1
        );
        assert!(buffer.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
        assert!(buffer.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 0));
        dk_figure_free(handle);
    }

    #[test]
    fn rotating_a_joint_moves_the_hand() {
        let handle = dk_figure_new(0);
        if handle.is_null() {
            return;
        }
        let mut before = [0.0_f32; 3];
        assert_eq!(
            dk_figure_joint(handle, Joint::HandL.index() as u32, before.as_mut_ptr()),
            1
        );
        assert_eq!(
            dk_figure_rotate(handle, Joint::ForearmL.index() as u32, 0.0, 0.0, 1.0, 1.0),
            1
        );
        let mut after = [0.0_f32; 3];
        assert_eq!(
            dk_figure_joint(handle, Joint::HandL.index() as u32, after.as_mut_ptr()),
            1
        );
        assert!((Vec3::from_array(after) - Vec3::from_array(before)).length() > 0.05);
        dk_figure_free(handle);
    }

    #[test]
    fn the_light_rig_changes_the_shade() {
        let handle = dk_figure_new(0);
        if handle.is_null() {
            return;
        }
        let mut lit = vec![0_u8; 32 * 48 * 4];
        let mut flat = vec![0_u8; 32 * 48 * 4];
        assert_eq!(
            dk_figure_render(handle, 32, 48, 0.2, 0.1, 4.5, lit.as_mut_ptr()),
            1
        );
        assert_eq!(
            dk_figure_set_lighting(
                handle, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            ),
            1
        );
        assert_eq!(
            dk_figure_render(handle, 32, 48, 0.2, 0.1, 4.5, flat.as_mut_ptr()),
            1
        );
        let brightness = |pixels: &[u8]| {
            let covered: Vec<u32> = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] == 255)
                .map(|pixel| pixel[0] as u32 + pixel[1] as u32 + pixel[2] as u32)
                .collect();
            covered.iter().sum::<u32>() / covered.len().max(1) as u32
        };
        assert!(brightness(&lit) > brightness(&flat));
        dk_figure_free(handle);
    }

    #[test]
    fn an_unknown_joint_is_rejected_instead_of_panicking() {
        let handle = dk_figure_new(3);
        if handle.is_null() {
            return;
        }
        let mut out = [0.0_f32; 3];
        assert_eq!(dk_figure_joint(handle, 99, out.as_mut_ptr()), 0);
        assert_eq!(dk_figure_rotate(handle, 99, 0.0, 1.0, 0.0, 0.5), 0);
        assert_eq!(
            dk_figure_pick(handle, 10.0, 10.0, 10.0, 1.0, 0.0, 0.0, 0.1),
            -1
        );
        dk_figure_free(handle);
    }
}
