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
    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
    assert!((out[0] as i32 - 127).abs() <= 2, "red {}", out[0]);
    assert!((out[2] as i32 - 128).abs() <= 2, "blue {}", out[2]);

    // Change the bottom layer without touching the top one; the new pixels must be used.
    let green = solid(2, 2, [0, 255, 0, 255]);
    assert!(stack.set(renderer.device(), renderer.queue(), 0, &green));
    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
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
    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0), fill]);
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
    for (resident, uploaded) in resident.iter().zip(&uploaded) {
        assert!(
            (resident - uploaded).abs() < 1e-5,
            "thumbnail mismatch: {resident} vs {uploaded}"
        );
    }
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
    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0), top]);
    assert!(out[0] > 250 && out[2] < 5, "hidden layer showed: {out:?}");

    stack.remove(1);
    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
    assert!(out[0] > 250 && out[2] < 5, "removed layer showed: {out:?}");
}

#[test]
fn a_cold_tile_is_rehydrated_from_its_backup() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (2048, 1);
    let mut stack = LayerStack::new(width, height);
    stack.set_budget((1024 * 4) as usize);
    assert!(stack.set(
        renderer.device(),
        renderer.queue(),
        0,
        &solid(width, height, [255, 0, 0, 255])
    ));
    assert!(stack.set(
        renderer.device(),
        renderer.queue(),
        1,
        &solid(width, height, [0, 0, 255, 128])
    ));
    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0), LayerSpec::new(1)]);
    assert_eq!(out.len(), (width * height * 4) as usize);
    assert!((out[0] as i32 - 127).abs() <= 2, "first tile {}", out[0]);
    let last = (width as usize - 1) * 4;
    assert!(
        (out[last] as i32 - 127).abs() <= 2,
        "second tile did not rehydrate: {}",
        out[last]
    );
    assert!(
        (out[last + 2] as i32 - 128).abs() <= 2,
        "blue {}",
        out[last + 2]
    );
}

#[test]
fn a_rect_across_a_tile_boundary_updates_both_tiles() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (2048, 1);
    let mut stack = LayerStack::new(width, height);
    assert!(stack.update_rect(
        renderer.device(),
        renderer.queue(),
        RectUpload {
            index: 0,
            x: 1000,
            y: 0,
            width: 48,
            height: 1,
            pixels: &solid(48, 1, [0, 255, 0, 255]),
        }
    ));
    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    assert!(out[1000 * 4 + 1] > 250, "left tile missing");
    assert!(out[1047 * 4 + 1] > 250, "right tile missing");
    assert!(out[999 * 4] == 0, "paint leaked left");
    assert!(out[2047 * 4] == 0, "paint leaked right");
}

#[test]
fn a_cached_composite_reuses_the_unpainted_tiles() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (width, height) = (2048, 1);
    let mut stack = LayerStack::new(width, height);
    assert!(stack.update_rect(
        renderer.device(),
        renderer.queue(),
        RectUpload {
            index: 0,
            x: 10,
            y: 0,
            width: 8,
            height: 1,
            pixels: &solid(8, 1, [255, 0, 0, 255]),
        }
    ));
    let first = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    let second = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    assert_eq!(first, second, "the cached output changed");
    assert!(first[10 * 4] > 250);

    // A later stroke must keep the earlier tile and update only the new one.
    assert!(stack.update_rect(
        renderer.device(),
        renderer.queue(),
        RectUpload {
            index: 0,
            x: 1500,
            y: 0,
            width: 8,
            height: 1,
            pixels: &solid(8, 1, [0, 0, 255, 255]),
        }
    ));
    let third = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    assert!(third[10 * 4] > 250, "the first stroke was lost");
    assert!(third[1500 * 4 + 2] > 250, "the second stroke is missing");
}

#[test]
fn a_clipped_layer_uses_the_base_alpha() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let mut stack = LayerStack::new(2, 2);
    let mut base = vec![0u8; 2 * 2 * 4];
    base[0..4].copy_from_slice(&[255, 0, 0, 255]);
    let top = solid(2, 2, [0, 0, 255, 255]);
    assert!(stack.set(renderer.device(), renderer.queue(), 0, &base));
    assert!(stack.set(renderer.device(), renderer.queue(), 1, &top));
    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0), LayerSpec::new(1).clip(0)]);
    assert!(out[2] > 250, "the clipped pixel did not show: {out:?}");
    assert_eq!(&out[4..8], &[0, 0, 0, 0], "the clip leaked at 1,0");
}

/// Copy a half-float texture back and decode the first pixel.
fn read_half_pixel(renderer: &Renderer, texture: &wgpu::Texture) -> [f32; 4] {
    let buffer = renderer.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("half readback"),
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .device()
        .create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue().submit(Some(encoder.finish()));
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = renderer.device().poll(wgpu::PollType::wait_indefinitely());
    let mapped = buffer.slice(..).get_mapped_range().expect("half map");
    let mut pixel = [0.0f32; 4];
    for (channel, value) in pixel.iter_mut().enumerate() {
        let bits = u16::from_le_bytes([mapped[channel * 2], mapped[channel * 2 + 1]]);
        *value = half::f16::from_bits(bits).to_f32();
    }
    drop(mapped);
    buffer.unmap();
    pixel
}

#[test]
fn an_extended_range_stack_keeps_highlights() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let mut stack = LayerStack::with_format(1, 1, wgpu::TextureFormat::Rgba16Float);
    // Straight linear Display P3, above paper white on the red channel.
    assert!(stack.set_linear(
        renderer.device(),
        renderer.queue(),
        0,
        &[2.0, 0.0, 0.0, 1.0]
    ));
    assert!(stack.set_linear(
        renderer.device(),
        renderer.queue(),
        1,
        &[0.0, 0.0, 2.0, 0.25]
    ));
    let pipeline =
        crate::composite::pipeline_for(renderer.device(), wgpu::TextureFormat::Rgba16Float);
    let texture = stack
        .composite_texture(
            renderer.device(),
            renderer.queue(),
            &pipeline,
            &[LayerSpec::new(0), LayerSpec::new(1)],
        )
        .expect("a composed texture");
    let pixel = read_half_pixel(&renderer, &texture);
    assert!((pixel[0] - 1.5).abs() < 0.01, "red {}", pixel[0]);
    assert!((pixel[2] - 0.5).abs() < 0.01, "blue {}", pixel[2]);
    assert!((pixel[3] - 1.0).abs() < 0.01, "alpha {}", pixel[3]);
}
