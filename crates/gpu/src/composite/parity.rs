use super::*;
use crate::Renderer;
use drawkit_core::document::{BlendMode, Document};
use drawkit_core::surface::Rect;

fn premultiplied(straight: &[[f32; 4]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(straight.len() * 4);
    for pixel in straight {
        let alpha = pixel[3].clamp(0.0, 1.0);
        out.push((pixel[0].clamp(0.0, 1.0) * alpha * 255.0).round() as u8);
        out.push((pixel[1].clamp(0.0, 1.0) * alpha * 255.0).round() as u8);
        out.push((pixel[2].clamp(0.0, 1.0) * alpha * 255.0).round() as u8);
        out.push((alpha * 255.0).round() as u8);
    }
    out
}

fn gpu_layers<'a>(document: &Document, buffers: &'a [Vec<u8>]) -> Vec<Layer<'a>> {
    document
        .layers
        .iter()
        .zip(buffers.iter())
        .map(|(layer, bytes)| Layer {
            pixels: bytes,
            opacity: layer.opacity,
            visible: layer.visible,
            blend: Blend::from_i32(layer.blend.as_i32()),
        })
        .collect()
}

fn layer_buffers(document: &Document) -> Vec<Vec<u8>> {
    let (width, height) = (document.width(), document.height());
    document
        .layers
        .iter()
        .map(|layer| {
            let mut bytes = Vec::with_capacity(width * height * 4);
            for y in 0..height {
                for x in 0..width {
                    for channel in layer.surface.get(x, y) {
                        bytes.push((channel * 255.0).round() as u8);
                    }
                }
            }
            bytes
        })
        .collect()
}

#[test]
fn the_gpu_compositor_matches_the_document() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let width = 8;
    let height = 8;
    let mut document = Document::new(width, height);
    document.layers[0].surface.set(2, 2, [1.0, 0.1, 0.1, 1.0]);
    document.layers[0].surface.set(3, 4, [0.2, 0.6, 0.3, 0.5]);
    document.add_layer("ink");
    document.active_mut().opacity = 0.6;
    document
        .active_mut()
        .surface
        .set(2, 2, [0.1, 0.2, 1.0, 0.8]);
    document
        .active_mut()
        .surface
        .set(6, 6, [0.9, 0.8, 0.1, 1.0]);
    document.add_layer("hidden");
    document.active_mut().visible = false;
    document
        .active_mut()
        .surface
        .set(0, 0, [1.0, 1.0, 1.0, 1.0]);

    let buffers = layer_buffers(&document);
    let layers = gpu_layers(&document, &buffers);
    let gpu = renderer.composite(&layers, width as u32, height as u32);
    let cpu = premultiplied(&document.composite_rect(Rect::new(0, 0, width, height)));
    assert_eq!(gpu.len(), cpu.len());
    for (index, (left, right)) in gpu.iter().zip(cpu.iter()).enumerate() {
        assert!(
            (*left as i32 - *right as i32).abs() <= 2,
            "byte {index}: gpu {left}, cpu {right}"
        );
    }
}

#[test]
fn every_blend_mode_matches_the_document() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let width = 4;
    let height = 4;
    let modes = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Add,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];
    for mode in modes {
        let mut document = Document::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let t = (y * width + x) as f32 / 15.0;
                document.layers[0]
                    .surface
                    .set(x, y, [0.15 + t * 0.5, 0.4 - t * 0.2, 0.3, 1.0]);
            }
        }
        document.add_layer("top");
        document.active_mut().blend = mode;
        for y in 0..height {
            for x in 0..width {
                let t = (x + y) as f32 / 8.0;
                document
                    .active_mut()
                    .surface
                    .set(x, y, [0.8 - t * 0.3, 0.25 + t * 0.4, 0.6, 0.7]);
            }
        }
        let buffers = layer_buffers(&document);
        let layers = gpu_layers(&document, &buffers);
        let gpu = renderer.composite(&layers, width as u32, height as u32);
        let cpu = premultiplied(&document.composite_rect(Rect::new(0, 0, width, height)));
        for (index, (left, right)) in gpu.iter().zip(cpu.iter()).enumerate() {
            assert!(
                (*left as i32 - *right as i32).abs() <= 2,
                "{mode:?} byte {index}: gpu {left}, cpu {right}"
            );
        }
    }
}
