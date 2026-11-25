// Reading a rectangle back from the resident tiles. A materialise moves these bytes into the CPU
// cache, so a wrong row stride here corrupts whatever is saved or exported later.

use super::*;
use crate::Renderer;

#[test]
fn a_rectangle_read_keeps_its_rows_and_columns() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (40u32, 8u32);
    let mut source = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            source.extend_from_slice(&[x as u8 * 6, y as u8 * 30, 0, 255]);
        }
    }
    let mut stack = LayerStack::new(width, height);
    assert!(stack.set(renderer.device(), renderer.queue(), 0, &source));

    let (rx, ry, rw, rh) = (4u32, 2u32, 20u32, 4u32);
    let tile = stack
        .read_layer_rect(renderer.device(), renderer.queue(), 0, (rx, ry, rw, rh))
        .expect("a tile");
    for row in 0..rh as usize {
        for column in 0..rw as usize {
            let at = (row * rw as usize + column) * 4;
            let pixel = (rx as usize + column, ry as usize + row);
            let expected = [pixel.0 as u8 * 6, pixel.1 as u8 * 30, 0, 255];
            assert_eq!(
                &tile.pixels[at..at + 4],
                &expected,
                "pixel {:?} row {row} column {column}",
                pixel
            );
        }
    }
}
