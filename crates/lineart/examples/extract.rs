//! Extract a line layer from an image with the embedded TEED model.
//!
//!   cargo run --release -p drawkit-lineart --example extract -- input.jpg output.png

use std::env;

use drawkit_lineart::{LineArtOptions, extract};
use image::RgbaImage;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: extract <input> <output.png>");
        return;
    }
    let image = image::open(&args[0]).expect("input image").to_rgba8();
    let (width, height) = image.dimensions();
    let lines = extract(
        image.as_raw(),
        width as usize,
        height as usize,
        LineArtOptions::default(),
    )
    .expect("the model ran");
    let output = RgbaImage::from_raw(width, height, lines).expect("rgba output");
    output.save(&args[1]).expect("output image");
}
