// The preview has to land between the layers around the stroke, not over the finished frame.

use wgpu::util::DeviceExt;

use crate::Renderer;

fn quad(pixel: [u8; 4], count: usize) -> Vec<u8> {
    pixel.repeat(count)
}

#[test]
fn a_stroke_previews_between_the_layers_around_it() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let mut stack = crate::LayerStack::new(2, 1);
    assert!(stack.set(device, queue, 0, &quad([0, 0, 255, 255], 2)));
    assert!(stack.set(device, queue, 1, &quad([0, 0, 0, 0], 2)));
    assert!(stack.set(device, queue, 2, &quad([0, 255, 0, 255], 2)));
    let mut specs: Vec<_> = (0..3).map(crate::LayerSpec::new).collect();
    // The layer above the stroke is translucent, so the red stroke changes what shows through.
    specs[2].opacity = 0.5;
    let pipeline = crate::composite::pipeline(device);
    let before = stack.composite(device, queue, &pipeline, &specs);
    assert!(before[0] < 2, "blue under green, red {}", before[0]);

    let coverage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("test coverage"),
        contents: bytemuck::cast_slice(&[1.0f32, 1.0]),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let tile_pipeline = crate::layers::commit_pipeline(device);
    let texture = stack
        .composite_stroke_preview(
            device,
            queue,
            &pipeline,
            &specs,
            1,
            (0, 0, 2, 1),
            &coverage,
            &tile_pipeline,
            [1.0, 0.0, 0.0, 1.0],
            0.0,
            0,
            [0.0; 2],
        )
        .expect("the preview composes");
    let after = crate::readback::read_texture(device, queue, &texture, 2, 1);
    assert!(
        after[0] > 100,
        "red stroke shows under green, red {}",
        after[0]
    );
    assert!(
        after[1] > 100,
        "green still covers half, green {}",
        after[1]
    );
}

#[test]
fn a_preview_stroke_drives_the_clip_of_the_layer_above_it() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let mut stack = crate::LayerStack::new(2, 1);
    assert!(stack.set(device, queue, 0, &quad([0, 0, 0, 0], 2)));
    assert!(stack.set(device, queue, 1, &quad([255, 0, 0, 255], 2)));
    let mut specs: Vec<_> = (0..2).map(crate::LayerSpec::new).collect();
    specs[1].clip = Some(0);
    let pipeline = crate::composite::pipeline(device);
    let coverage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("test coverage"),
        contents: bytemuck::cast_slice(&[1.0f32, 0.0]),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let tile_pipeline = crate::layers::commit_pipeline(device);
    let texture = stack
        .composite_stroke_preview(
            device,
            queue,
            &pipeline,
            &specs,
            0,
            (0, 0, 2, 1),
            &coverage,
            &tile_pipeline,
            [1.0, 1.0, 1.0, 1.0],
            0.0,
            0,
            [0.0; 2],
        )
        .expect("the preview composes");
    let pixels = crate::readback::read_texture(device, queue, &texture, 2, 1);
    assert_eq!(pixels[3], 255, "the stroke opens the clip on pixel 0");
    assert_eq!(pixels[7], 0, "pixel 1 stays outside the clip");
}

#[test]
fn a_group_still_applies_its_opacity_to_the_preview() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let mut stack = crate::LayerStack::new(1, 1);
    assert!(stack.set(device, queue, 0, &quad([0, 0, 0, 0], 1)));
    assert!(stack.set(device, queue, 1, &quad([0, 0, 0, 0], 1)));
    let mut specs: Vec<_> = (0..2).map(crate::LayerSpec::new).collect();
    specs[0].opacity = 0.5;
    specs[1].parent = Some(0);
    let pipeline = crate::composite::pipeline(device);
    let coverage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("test coverage"),
        contents: bytemuck::cast_slice(&[1.0f32]),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let tile_pipeline = crate::layers::commit_pipeline(device);
    let texture = stack
        .composite_stroke_preview(
            device,
            queue,
            &pipeline,
            &specs,
            1,
            (0, 0, 1, 1),
            &coverage,
            &tile_pipeline,
            [1.0, 0.0, 0.0, 1.0],
            0.0,
            0,
            [0.0; 2],
        )
        .expect("the preview composes");
    let pixels = crate::readback::read_texture(device, queue, &texture, 1, 1);
    assert!(
        (100..=155).contains(&pixels[3]),
        "group alpha {}",
        pixels[3]
    );
    assert!((100..=155).contains(&pixels[0]), "group red {}", pixels[0]);
}

#[test]
fn an_extended_range_preview_keeps_its_highlights() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let format = wgpu::TextureFormat::Rgba16Float;
    let mut stack = crate::LayerStack::with_format(1, 1, crate::FrameFormat::Rgba16);
    assert!(stack.set_linear(device, queue, 0, &[0.0, 0.0, 0.0, 0.0]));
    let specs = [crate::LayerSpec::new(0)];
    let pipeline = crate::composite::pipeline_for(device, format);
    let coverage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("test coverage"),
        contents: bytemuck::cast_slice(&[1.0f32]),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let tile_pipeline = crate::layers::commit_pipeline_for(device, format);
    let texture = stack
        .composite_stroke_preview(
            device,
            queue,
            &pipeline,
            &specs,
            0,
            (0, 0, 1, 1),
            &coverage,
            &tile_pipeline,
            [2.0, 0.5, 0.25, 1.0],
            0.0,
            0,
            [0.0; 2],
        )
        .expect("the preview composes");
    let pixels = crate::readback::read_texture_linear(device, queue, &texture, 1, 1);
    assert!(
        (pixels[0] - 2.0).abs() < 0.05,
        "highlight red {}",
        pixels[0]
    );
    assert!((pixels[1] - 0.5).abs() < 0.02, "green {}", pixels[1]);
}

#[test]
fn a_half_float_layer_thumbnail_encodes_linear_pixels() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let mut stack = crate::LayerStack::with_format(1, 1, crate::FrameFormat::Rgba16);
    assert!(stack.set_linear(device, queue, 0, &[0.214, 0.0, 0.0, 1.0]));
    let pixels = stack.thumbnail(0, 1, 1);
    assert!((pixels[0] - 0.5).abs() < 0.01, "encoded red {}", pixels[0]);
    assert!((pixels[3] - 1.0).abs() < 0.01, "alpha {}", pixels[3]);
}

#[test]
fn a_layer_thumbnail_premultiplies_and_skips_paper() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let mut stack = crate::LayerStack::new(2, 1);
    assert!(stack.set(device, queue, 0, &quad([255, 0, 0, 128], 2)));
    let pixels = stack.thumbnail(0, 2, 1);
    assert_eq!(pixels.len(), 8);
    assert!(
        (pixels[0] - 0.5).abs() < 0.01,
        "premultiplied red {}",
        pixels[0]
    );
    assert!((pixels[3] - 0.5).abs() < 0.01, "alpha {}", pixels[3]);
    assert!(stack.thumbnail(9, 2, 1).is_empty(), "no such layer");
}
