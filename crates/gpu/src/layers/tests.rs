use super::*;
use crate::Renderer;

fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    pixel
        .iter()
        .copied()
        .cycle()
        .take((width * height * 4) as usize)
        .collect()
}

#[test]
fn a_resident_stack_composites_and_updates() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let mut stack = LayerStack::new(2, 2);
    let red = solid(2, 2, [255, 0, 0, 255]);
    let blue = solid(2, 2, [0, 0, 255, 128]);
    assert!(stack.set(renderer.device(), renderer.queue(), 0, &red));
    assert!(stack.set(renderer.device(), renderer.queue(), 1, &blue));
    let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
    assert!((out[0] as i32 - 127).abs() <= 2, "red {}", out[0]);
    assert!((out[2] as i32 - 128).abs() <= 2, "blue {}", out[2]);

    // Change the bottom layer without touching the top one; the new pixels must be used.
    let green = solid(2, 2, [0, 255, 0, 255]);
    assert!(stack.set(renderer.device(), renderer.queue(), 0, &green));
    let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
    assert!(out[1] > 100, "the bottom layer did not update: {}", out[1]);
    assert!(out[0] < 130, "red did not fall: {}", out[0]);
}

#[test]
fn a_fill_spec_composites_without_a_texture() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let mut stack = LayerStack::new(2, 2);
    stack.set(
        renderer.device(),
        renderer.queue(),
        0,
        &solid(2, 2, [255, 0, 0, 255]),
    );
    let mut fill = LayerSpec::new(1).fill([0.0, 0.0, 1.0, 0.5]);
    fill.opacity = 1.0;
    let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), fill]);
    assert!((out[0] as i32 - 127).abs() <= 2, "red {}", out[0]);
    assert!((out[2] as i32 - 128).abs() <= 2, "blue {}", out[2]);
}

#[test]
fn a_resident_layer_can_make_its_thumbnail() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (8u32, 6u32);
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let index = (y * width as usize + x) * 4;
            pixels[index] = (x * 30) as u8;
            pixels[index + 1] = (y * 40) as u8;
            pixels[index + 2] = 90;
            pixels[index + 3] = if x < 4 { 255 } else { 80 };
        }
    }
    let mut stack = LayerStack::new(width, height);
    stack.set(renderer.device(), renderer.queue(), 0, &pixels);
    let resident = renderer.layer_thumbnail(&stack, 0, 4, 3);
    let uploaded = renderer.thumbnail(&pixels, width, height, 4, 3);
    assert_eq!(resident, uploaded);
}

#[test]
fn a_hidden_or_removed_layer_leaves_the_composite() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let mut stack = LayerStack::new(2, 2);
    stack.set(
        renderer.device(),
        renderer.queue(),
        0,
        &solid(2, 2, [255, 0, 0, 255]),
    );
    stack.set(
        renderer.device(),
        renderer.queue(),
        1,
        &solid(2, 2, [0, 0, 255, 255]),
    );
    let mut top = LayerSpec::new(1);
    top.visible = false;
    let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), top]);
    assert!(out[0] > 250 && out[2] < 5, "hidden layer showed: {out:?}");

    stack.remove(1);
    let out = renderer.composite_stack(&stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
    assert!(out[0] > 250 && out[2] < 5, "removed layer showed: {out:?}");
}

#[test]
fn a_budget_drops_the_cold_layer() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let mut stack = LayerStack::new(2, 2);
    stack.set_budget(64 << 20);
    assert!(stack.set(
        renderer.device(),
        renderer.queue(),
        0,
        &solid(2, 2, [255, 0, 0, 255])
    ));
    assert!(!stack.layer_missing(0));
    // Two textures fit; a fourth upload on a tiny budget must evict a cold one.
    stack.set_budget(2 * 2 * 4 * 2);
    assert!(stack.set(
        renderer.device(),
        renderer.queue(),
        1,
        &solid(2, 2, [0, 0, 255, 255])
    ));
    assert!(!stack.layer_missing(1));
    assert!(stack.set(
        renderer.device(),
        renderer.queue(),
        2,
        &solid(2, 2, [0, 255, 0, 255])
    ));
    assert!(!stack.layer_missing(2));
    assert!(stack.resident_bytes() <= stack.budget());
}
