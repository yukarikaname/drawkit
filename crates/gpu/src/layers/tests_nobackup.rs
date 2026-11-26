// The live canvas keeps no CPU tile copy: an evicted tile is a hole the shell fills again.

use wgpu::util::DeviceExt;

use super::*;
use crate::{FrameFormat, Renderer};

fn solid(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    pixel
        .iter()
        .copied()
        .cycle()
        .take((width * height * 4) as usize)
        .collect()
}

fn rgba(width: u32, pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
}

#[test]
fn a_backup_less_stack_holds_textures_only() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let (width, height) = (2048u32, 1024u32);
    let mut stack = LayerStack::without_backups(width, height, crate::FrameFormat::Rgba8);
    assert!(!stack.keeps_backups());
    assert!(stack.set(device, queue, 0, &solid(width, height, [20, 40, 60, 255])));

    // Two 1024 tile textures and not one byte of backup.
    let tile = 1024 * 1024 * 4;
    assert_eq!(stack.resident_bytes(), 2 * tile);
    assert_eq!(stack.backup_bytes(), 0);
    assert!(stack.missing_rects().is_empty());

    // The CPU-only previews refuse instead of allocating a backup on demand.
    assert!(stack.thumbnail(0, 4, 4).is_empty());
    let mut mask = vec![0.0f32; (width * height) as usize];
    mask[0] = 1.0;
    assert!(!stack.apply_fill_mask(0, &mask, [1.0, 0.0, 0.0, 1.0], false, false));

    // The compositor mode is unchanged: eviction is recovered from the backup, not reported.
    let mut backed = LayerStack::new(width, height);
    assert!(backed.keeps_backups());
    assert!(backed.set(device, queue, 0, &solid(width, height, [1, 2, 3, 255])));
    assert!(backed.backup_bytes() > 0);
    backed.set_budget(tile);
    assert!(backed.missing_rects().is_empty());
}

#[test]
fn an_evicted_tile_is_reported_empty_then_upload_restores_it() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let (width, height) = (2048u32, 1024u32);
    let mut stack = LayerStack::without_backups(width, height, crate::FrameFormat::Rgba8);
    assert!(stack.set(device, queue, 0, &solid(width, height, [255, 0, 0, 255])));
    let tile = 1024 * 1024 * 4;
    stack.set_budget(tile);

    let missing = stack.missing_rects();
    assert_eq!(missing.len(), 1, "the budget must drop exactly one tile");
    let (layer, x, y, w, h) = missing[0];
    assert_eq!(layer, 0);
    assert_eq!((w, h), (1024, 1024));

    // A GPU-to-CPU read refuses the hole instead of reporting zeros over the core surface.
    assert!(
        stack
            .read_layer_rect(device, queue, layer, (x, y, w, h))
            .is_none(),
        "a missing tile was materialised as empty"
    );

    // The evicted tile composites as nothing; the resident one keeps its pixels.
    let pixels = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    assert_eq!(
        rgba(width, &pixels, x + 10, y + 10),
        [0, 0, 0, 0],
        "an evicted tile drew something"
    );
    let resident_x = if x == 0 { 1024 } else { 0 };
    assert_eq!(
        rgba(width, &pixels, resident_x + 10, y + 10),
        [255, 0, 0, 255],
        "the resident tile lost its pixels"
    );

    // A shell re-upload of the reported rectangle brings the pixels back. The budget must be
    // large enough for both tiles, or the upload itself evicts the resident one.
    stack.set_budget(2 * tile);
    let blue = solid(w, h, [0, 0, 255, 255]);
    assert!(stack.update_rect(
        device,
        queue,
        RectUpload {
            index: layer,
            x,
            y,
            width: w,
            height: h,
            pixels: &blue,
        }
    ));
    assert!(
        stack.missing_rects().is_empty(),
        "the tile is resident again"
    );
    assert_eq!(stack.resident_bytes(), 2 * tile);
    let pixels = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    assert_eq!(rgba(width, &pixels, x + 10, y + 10), [0, 0, 255, 255]);
}

#[test]
fn undo_and_redo_swap_correctly_with_an_evicted_layer() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let (width, height) = (2048u32, 1u32);
    let mut stack = LayerStack::without_backups(width, height, crate::FrameFormat::Rgba8);
    assert!(stack.set(device, queue, 0, &solid(width, height, [255, 0, 0, 255])));
    assert!(stack.set(device, queue, 1, &solid(width, height, [0, 0, 255, 255])));

    let coverage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("no-backup commit coverage"),
        contents: bytemuck::cast_slice(&vec![1.0f32; (width * height) as usize]),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let pipeline = crate::layers::commit_pipeline(device);
    let mut encoder = device.create_command_encoder(&Default::default());
    let (_, swaps) = stack.commit_stroke(
        device,
        queue,
        &mut encoder,
        &pipeline,
        0,
        &coverage,
        (0, 0, width, height),
        [0.0, 1.0, 0.0],
        1.0,
        0.0,
        0,
        [0.0; 2],
    );
    queue.submit(Some(encoder.finish()));
    assert_eq!(swaps.len(), 2, "the stroke touched both tiles");

    // The journal pins the tiles whose live texture undo/redo will exchange. This is what the
    // surface does after every push, so eviction may only take the other layer's cold tiles.
    let pinned: Vec<_> = swaps.iter().map(|swap| (swap.layer, swap.coord)).collect();
    let tile = 1024 * 4;
    stack.set_pinned(pinned);
    stack.set_budget(2 * tile);
    let missing = stack.missing_rects();
    assert_eq!(missing.len(), 2, "the unpinned layer should be evicted");
    assert!(missing.iter().all(|(layer, ..)| *layer == 1));

    // Undo and redo still swap the pinned layer's textures with its journal.
    let mut redo = Vec::new();
    for swap in swaps {
        redo.push(stack.restore_swap(swap));
    }
    let pixels = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    for x in [10, 1500] {
        assert_eq!(rgba(width, &pixels, x, 0), [255, 0, 0, 255], "undo at {x}");
    }

    for swap in redo {
        stack.restore_swap(swap);
    }
    let pixels = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    for x in [10, 1500] {
        assert_eq!(rgba(width, &pixels, x, 0), [0, 255, 0, 255], "redo at {x}");
    }
    assert_eq!(stack.missing_rects(), missing, "undo/redo touched layer 1");
}

#[test]
fn a_resident_layer_draws_its_own_thumbnail() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let side = 1024;
    let mut stack = LayerStack::without_backups(side, side, FrameFormat::Rgba8);
    assert!(stack.insert_layer(0, side, side));
    // One red corner tile; the rest of the layer is untouched.
    let pixels = solid(256, 256, [255, 0, 0, 255]);
    assert!(stack.update_rect(
        device,
        queue,
        RectUpload {
            index: 0,
            x: 0,
            y: 0,
            width: 256,
            height: 256,
            pixels: &pixels,
        }
    ));
    let thumb = stack
        .layer_thumbnail(device, queue, 0, 8, 8)
        .expect("a resident thumbnail");
    assert_eq!(thumb.len(), 8 * 8 * 4);
    let red = rgba(8, &thumb, 1, 1);
    assert!(
        red[0] > 200 && red[1] < 40 && red[3] > 200,
        "the painted corner is red: {red:?}"
    );
    assert_eq!(rgba(8, &thumb, 7, 7)[3], 0, "the rest stays transparent");

    // An empty layer is fully transparent, and a backup-keeping stack keeps the CPU path.
    assert!(stack.insert_layer(1, side, side));
    let empty = stack
        .layer_thumbnail(device, queue, 1, 8, 8)
        .expect("a thumbnail");
    assert!(empty.iter().all(|value| *value == 0));
    let mut with_backups = LayerStack::new(side, side);
    with_backups.insert_layer(0, side, side);
    assert!(
        with_backups
            .layer_thumbnail(device, queue, 0, 8, 8)
            .is_none()
    );
}
