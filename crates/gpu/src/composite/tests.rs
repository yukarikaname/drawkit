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
fn a_half_transparent_layer_blends_over_the_one_below() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let red = solid(2, 2, [255, 0, 0, 255]);
    let blue = solid(2, 2, [0, 0, 255, 128]);
    let out = renderer.composite(&[Layer::new(&red, 1.0), Layer::new(&blue, 1.0)], 2, 2);
    assert_eq!(out.len(), 16);
    let pixel = &out[..4];
    assert!((pixel[0] as i32 - 127).abs() <= 2, "red {}", pixel[0]);
    assert!(pixel[1] <= 2, "green {}", pixel[1]);
    assert!((pixel[2] as i32 - 128).abs() <= 2, "blue {}", pixel[2]);
    assert_eq!(pixel[3], 255);
}

#[test]
fn opacity_and_visibility_are_honoured() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let red = solid(4, 4, [255, 0, 0, 255]);
    let blue = solid(4, 4, [0, 0, 255, 255]);
    let hidden = Layer {
        pixels: &blue,
        opacity: 1.0,
        visible: false,
        blend: Blend::Normal,
    };
    let out = renderer.composite(
        &[Layer::new(&red, 1.0), hidden, Layer::new(&blue, 0.5)],
        4,
        4,
    );
    let pixel = &out[..4];
    assert!((pixel[0] as i32 - 127).abs() <= 2, "red {}", pixel[0]);
    assert!((pixel[2] as i32 - 128).abs() <= 2, "blue {}", pixel[2]);
}

#[test]
fn an_empty_stack_is_transparent() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let out = renderer.composite(&[], 2, 2);
    assert_eq!(out, vec![0; 16]);
}

#[test]
fn multiply_darkens_the_backdrop() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let grey = solid(2, 2, [128, 128, 128, 255]);
    let half = solid(2, 2, [128, 128, 128, 255]);
    let out = renderer.composite(
        &[
            Layer::new(&grey, 1.0),
            Layer::new(&half, 1.0).blended(Blend::Multiply),
        ],
        2,
        2,
    );
    assert!((out[0] as i32 - 64).abs() <= 2, "red {}", out[0]);
    assert_eq!(out[3], 255);
}
