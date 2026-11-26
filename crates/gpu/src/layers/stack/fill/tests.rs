//! Fill journaling, GPU parity and residency, plus the full-canvas benchmark.

use crate::{FrameFormat, LayerSpec, LayerStack, Renderer};
use std::time::Instant;

fn empty_mask(width: u32, height: u32) -> Vec<f32> {
    vec![0.0; width as usize * height as usize]
}

/// A mask that covers a rectangle, the shape the core hands a live fill.
fn rect_mask(width: u32, height: u32, x: u32, y: u32, w: u32, h: u32) -> Vec<f32> {
    let mut mask = empty_mask(width, height);
    for row in y..(y + h) {
        for column in x..(x + w) {
            mask[row as usize * width as usize + column as usize] = 1.0;
        }
    }
    mask
}

type FillSwaps = (Vec<(u32, u32, u32, u32)>, Vec<crate::layers::TileSwap>);

/// Run a fill through the same GPU path the surface uses and submit it.
fn gpu_fill(
    stack: &mut LayerStack,
    renderer: &Renderer,
    layer: usize,
    mask: &[f32],
    color: [f32; 4],
    locked: bool,
    under: bool,
) -> FillSwaps {
    let format = stack.format();
    let pipeline = LayerStack::fill_pipeline(renderer.device(), format.texture());
    let mut encoder = renderer
        .device()
        .create_command_encoder(&Default::default());
    let result = stack.commit_fill(
        renderer.device(),
        renderer.queue(),
        &mut encoder,
        &pipeline,
        layer,
        mask,
        color,
        locked,
        under,
    );
    renderer.queue().submit(Some(encoder.finish()));
    result
}

#[test]
fn a_committed_fill_is_resident_and_undoable() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let (width, height) = (8, 8);
    let mut stack = LayerStack::new(width, height);
    assert!(stack.set(device, queue, 0, &vec![0u8; 8 * 8 * 4]));
    let mask = rect_mask(width, height, 2, 3, 2, 1);
    let (rects, swaps) = gpu_fill(
        &mut stack,
        &renderer,
        0,
        &mask,
        [1.0, 0.0, 0.0, 1.0],
        false,
        false,
    );
    assert_eq!(rects, vec![(2, 3, 2, 1)]);
    assert_eq!(swaps.len(), 1, "the replaced tile is the undo record");

    // The tile is already on the GPU, so a composite sees the fill before any rehydration.
    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    let at = (3 * width as usize + 2) * 4;
    assert!(out[at] > 200, "red {}", out[at]);
    assert!(out[at + 3] > 200, "alpha {}", out[at + 3]);
    let untouched = 0;
    assert_eq!(out[untouched + 3], 0, "outside the mask changed");

    // Undo swaps the pre-fill texture back; redo puts the filled one in.
    for swap in swaps {
        let redo = stack.restore_swap(swap);
        let tile = stack
            .read_layer_rect(device, queue, 0, (2, 3, 2, 1))
            .expect("a read-back tile");
        assert_eq!(tile.pixels[3], 0, "undo left the fill behind");
        stack.restore_swap(redo);
        let tile = stack
            .read_layer_rect(device, queue, 0, (2, 3, 2, 1))
            .expect("a read-back tile");
        assert!(tile.pixels[0] > 200, "redo lost the fill");
    }
}

#[test]
fn a_fill_under_keeps_the_line_on_top() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let (width, height) = (4, 4);
    let mut stack = LayerStack::new(width, height);
    // Opaque blue at (1, 1), the rest empty.
    let mut pixels = vec![0u8; 4 * 4 * 4];
    let at = (4 + 1) * 4;
    pixels[at + 2] = 255;
    pixels[at + 3] = 255;
    assert!(stack.set(device, queue, 0, &pixels));
    let mask = vec![1.0_f32; (width * height) as usize];
    let (_, swaps) = gpu_fill(
        &mut stack,
        &renderer,
        0,
        &mask,
        [1.0, 0.0, 0.0, 1.0],
        false,
        true,
    );
    assert_eq!(swaps.len(), 1);
    let tile = stack
        .read_layer_rect(device, queue, 0, (1, 1, 1, 1))
        .expect("a read-back tile");
    assert!(
        tile.pixels[2] > 200,
        "the line was covered: {:?}",
        tile.pixels
    );
}

/// The blend expression the CPU path uses, kept here so the GPU pass is checked against it.
/// `alpha` is already `amount * colour.alpha`.
fn cpu_blend(old: [f32; 4], color: [f32; 4], alpha: f32, locked: bool, under: bool) -> [f32; 4] {
    if locked {
        if old[3] <= 0.0 {
            return old;
        }
        return [
            old[0] + (color[0] - old[0]) * alpha,
            old[1] + (color[1] - old[1]) * alpha,
            old[2] + (color[2] - old[2]) * alpha,
            old[3],
        ];
    }
    if under {
        let out_alpha = old[3] + alpha * (1.0 - old[3]);
        if out_alpha <= 0.0 {
            return [0.0; 4];
        }
        return [
            (old[0] * old[3] + color[0] * alpha * (1.0 - old[3])) / out_alpha,
            (old[1] * old[3] + color[1] * alpha * (1.0 - old[3])) / out_alpha,
            (old[2] * old[3] + color[2] * alpha * (1.0 - old[3])) / out_alpha,
            out_alpha,
        ];
    }
    let out_alpha = alpha + old[3] * (1.0 - alpha);
    if out_alpha <= 0.0 {
        return [0.0; 4];
    }
    [
        (color[0] * alpha + old[0] * old[3] * (1.0 - alpha)) / out_alpha,
        (color[1] * alpha + old[1] * old[3] * (1.0 - alpha)) / out_alpha,
        (color[2] * alpha + old[2] * old[3] * (1.0 - alpha)) / out_alpha,
        out_alpha,
    ]
}

/// Source-over, under, alpha lock, a partial fill alpha and a soft mask edge. The GPU mask is
/// quantised to eight bits, so a pixel may differ from the CPU formula by one level.
#[test]
fn a_gpu_fill_matches_the_cpu_blend() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let (width, height) = (8u32, 8u32);
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let at = ((y * width + x) * 4) as usize;
            pixels[at] = (x * 31) as u8;
            pixels[at + 1] = (y * 27) as u8;
            pixels[at + 2] = ((x + y) * 13) as u8;
            pixels[at + 3] = match (x * 3 + y) % 5 {
                0 => 0,
                1 => 64,
                2 => 128,
                3 => 200,
                _ => 255,
            };
        }
    }
    let mut mask = empty_mask(width, height);
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            mask[(y * width + x) as usize] = if x == 1 || y == 1 || x == width - 2 {
                0.5
            } else {
                1.0
            };
        }
    }
    let colours = [
        [0.8, 0.2, 0.1, 1.0],
        [0.3, 0.6, 0.9, 0.5],
        [0.9, 0.9, 0.1, 0.25],
    ];
    for color in colours {
        for locked in [false, true] {
            for under in [false, true] {
                let mut cpu = LayerStack::new(width, height);
                assert!(cpu.set(device, queue, 0, &pixels));
                assert!(cpu.apply_fill_mask(0, &mask, color, locked, under));
                let expected = cpu.layers[0].as_ref().expect("layer").tiles[&(0, 0)]
                    .backup
                    .clone();

                let mut gpu = LayerStack::new(width, height);
                assert!(gpu.set(device, queue, 0, &pixels));
                let (rects, swaps) = gpu_fill(&mut gpu, &renderer, 0, &mask, color, locked, under);
                assert!(!rects.is_empty());
                assert_eq!(rects.len(), swaps.len());
                // Reading the tile is the lazy materialise path: it also refreshes the backup.
                let tile = gpu
                    .read_layer_rect(device, queue, 0, (0, 0, width, height))
                    .expect("a read-back tile");
                for index in 0..(width * height) as usize {
                    for channel in 0..4 {
                        let got = tile.pixels[index * 4 + channel];
                        let want = expected[index * 4 + channel];
                        assert!(
                            got.abs_diff(want) <= 1,
                            "colour {color:?} locked {locked} under {under} pixel {index} \
                             channel {channel}: {got} vs {want}"
                        );
                    }
                }
            }
        }
    }
}

/// The measured cost of a full-canvas fill, printed with `cargo test ... -- --ignored
/// --nocapture`. It is a benchmark, not a correctness test, so it stays out of the default run.
#[test]
#[ignore = "benchmark: prints fill apply time at 2K and 4K"]
fn a_full_fill_takes_a_bounded_time() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let pipeline = LayerStack::fill_pipeline(device, FrameFormat::Rgba8.texture());
    for (width, height) in [(2048_u32, 2048_u32), (4096, 4096)] {
        let area = width as usize * height as usize;
        let colour = [0.8, 0.2, 0.1, 1.0];

        // A real fill runs on a stack the shell already uploaded and presented, so every tile is
        // resident. Warming the stack keeps the cold texture uploads out of the numbers.
        let mut stack = LayerStack::new(width, height);
        assert!(stack.set(device, queue, 0, &vec![0u8; area * 4]));
        let _ = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);

        let build = Instant::now();
        let full = vec![1.0_f32; area];
        let build = build.elapsed();

        let apply = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        let (rects, swaps) = stack.commit_fill(
            device,
            queue,
            &mut encoder,
            &pipeline,
            0,
            &full,
            colour,
            false,
            false,
        );
        queue.submit(Some(encoder.finish()));
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let elapsed = apply.elapsed();
        assert!(!rects.is_empty());
        assert_eq!(rects.len(), swaps.len());
        println!(
            "fill {width}x{height}: mask build {:.1} ms, GPU fill {:.1} ms, {} tiles",
            build.as_secs_f64() * 1e3,
            elapsed.as_secs_f64() * 1e3,
            rects.len()
        );
    }
}

/// The CPU replay still runs `apply_fill_mask`, so its opaque fast path has to produce exactly the
/// same bytes as the general expression it replaced.
#[test]
fn the_opaque_fast_path_matches_the_general_blend() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let olds: [[u8; 4]; 4] = [
        [0, 0, 0, 0],
        [255, 0, 0, 128],
        [10, 20, 30, 255],
        [1, 2, 3, 7],
    ];
    let colours = [[0.8, 0.2, 0.1, 1.0], [0.3, 0.6, 0.9, 0.5]];
    for color in colours {
        for locked in [false, true] {
            for under in [false, true] {
                let mut stack = LayerStack::new(4, 1);
                let pixels: Vec<u8> = olds
                    .iter()
                    .flat_map(|pixel| pixel.iter().copied())
                    .collect();
                assert!(stack.set(device, queue, 0, &pixels));
                let mask = vec![1.0_f32; 4];
                assert!(stack.apply_fill_mask(0, &mask, color, locked, under));
                let backup = &stack.layers[0].as_ref().expect("layer").tiles[&(0, 0)].backup;
                for (index, old) in olds.iter().enumerate() {
                    let straight = [
                        f32::from(old[0]) / 255.0,
                        f32::from(old[1]) / 255.0,
                        f32::from(old[2]) / 255.0,
                        f32::from(old[3]) / 255.0,
                    ];
                    let expected = cpu_blend(straight, color, color[3], locked, under);
                    for channel in 0..4 {
                        let want = (expected[channel].clamp(0.0, 1.0) * 255.0).round() as u8;
                        assert_eq!(
                            backup[index * 4 + channel],
                            want,
                            "colour {color:?} locked {locked} under {under} old {old:?} \
                             channel {channel}"
                        );
                    }
                }
            }
        }
    }
}
