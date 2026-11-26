//! Wet baking: field-to-tile colour, dirty rectangles, idempotence and the 4K cost.

use crate::readback::{FramePixels, read_frame};
use crate::{FrameFormat, LayerSpec, LayerStack, Renderer, WetDeposit, WetField, WetSettings};
use std::time::Instant;

type WetResult = (Vec<(u32, u32, u32, u32)>, Vec<crate::layers::TileSwap>);

/// Run a bake through the same GPU path the surface uses and submit it.
fn bake(
    stack: &mut LayerStack,
    renderer: &Renderer,
    layer: usize,
    deposit: &WetDeposit,
    rect: (u32, u32, u32, u32),
) -> WetResult {
    let pipeline = LayerStack::wet_pipeline(renderer.device(), stack.format().texture());
    let mut encoder = renderer
        .device()
        .create_command_encoder(&Default::default());
    let result = stack.commit_wet(
        renderer.device(),
        renderer.queue(),
        &mut encoder,
        &pipeline,
        layer,
        deposit,
        rect,
    );
    renderer.queue().submit(Some(encoder.finish()));
    let _ = renderer.device().poll(wgpu::PollType::wait_indefinitely());
    result
}

fn pixel(image: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [image[at], image[at + 1], image[at + 2], image[at + 3]]
}

/// A wet disc on a half-resolution field deposits pigment into layer 0, and the layer composite
/// shows it.
#[test]
fn a_wet_disc_deposits_into_the_layer() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let (width, height) = (128u32, 128u32);
    // A faster absorption keeps the test short while still settling most of the disc.
    let settings = WetSettings {
        absorption: 0.5,
        ..WetSettings::default()
    };
    let mut field =
        WetField::for_canvas(device, queue, (width, height), 0.5, settings).expect("a wet field");
    field.inject(device, queue, (64, 64), 20.0, 1.0, [1.0, 0.0, 0.0]);
    field.advance(device, queue, 60);
    let deposit = field.deposit(device, queue).expect("a deposit");

    let mut stack = LayerStack::new(width, height);
    assert!(stack.set(device, queue, 0, &vec![0u8; (width * height * 4) as usize]));
    let rect = (44, 44, 40, 40);
    let (rects, swaps) = bake(&mut stack, &renderer, 0, &deposit, rect);
    assert_eq!(rects, vec![rect], "a small canvas is one tile");
    assert_eq!(swaps.len(), 1, "the replaced texture is the undo record");

    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    let centre = pixel(&out, width, 64, 64);
    assert!(centre[0] > 20, "the disc did not bake: {centre:?}");
    assert!(centre[3] > 20, "no coverage: {centre:?}");
    assert!(centre[1] < 10 && centre[2] < 10, "not red: {centre:?}");
    assert_eq!(pixel(&out, width, 0, 0)[3], 0, "the corner changed");
}

/// The returned rectangles are exactly the intersections of the request with the touched tiles,
/// and undo puts the transparent tiles back.
#[test]
fn dirty_rects_match_the_tiles_and_undo_restores_transparency() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let canvas = (2048u32, 2048u32);
    let mut field = WetField::for_canvas(device, queue, canvas, 0.5, WetSettings::default())
        .expect("a wet field");
    field.inject(device, queue, (1024, 1024), 256.0, 1.0, [0.1, 0.2, 0.9]);
    field.advance(device, queue, 30);
    let deposit = field.deposit(device, queue).expect("a deposit");

    let mut stack = LayerStack::new(canvas.0, canvas.1);
    assert!(stack.set(
        device,
        queue,
        0,
        &vec![0u8; (canvas.0 * canvas.1 * 4) as usize]
    ));
    // The request crosses the 1024-pixel tile boundary on both axes.
    let rect = (900, 900, 300, 300);
    let (rects, swaps) = bake(&mut stack, &renderer, 0, &deposit, rect);
    assert_eq!(
        rects,
        vec![
            (900, 900, 124, 124),
            (1024, 900, 176, 124),
            (900, 1024, 124, 176),
            (1024, 1024, 176, 176),
        ]
    );
    assert_eq!(rects.len(), swaps.len());

    let baked = stack
        .read_layer_rect(device, queue, 0, rect)
        .expect("a read-back tile");
    assert!(
        baked.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
        "the bake wrote nothing"
    );

    // Undo: every replaced texture goes back, so the whole rect is transparent again.
    let mut redo = Vec::with_capacity(swaps.len());
    for swap in swaps {
        redo.push(stack.restore_swap(swap));
    }
    let undone = stack
        .read_layer_rect(device, queue, 0, rect)
        .expect("a read-back tile");
    assert!(
        undone.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 0),
        "undo left pigment behind"
    );

    // Redo: the baked textures swap back in.
    for swap in redo {
        stack.restore_swap(swap);
    }
    let redone = stack
        .read_layer_rect(device, queue, 0, rect)
        .expect("a read-back tile");
    assert!(
        redone.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
        "redo lost the bake"
    );
}

/// A dry field's settled pigment is baked once; baking it again is the same pixels, not a darker
/// second film. A field that never stepped bakes nothing at all.
#[test]
fn a_dry_field_bakes_once_and_a_fresh_field_is_a_no_op() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let (width, height) = (64u32, 64u32);
    let settings = WetSettings {
        evaporation: 2.0,
        ..WetSettings::default()
    };

    let fresh = WetField::new(device, queue, width, height, settings).expect("a wet field");
    let idle = fresh.deposit(device, queue).expect("a deposit");
    assert!(idle.is_empty());
    let mut stack = LayerStack::new(width, height);
    assert!(stack.set(device, queue, 0, &vec![0u8; (width * height * 4) as usize]));
    let (rects, swaps) = bake(&mut stack, &renderer, 0, &idle, (0, 0, width, height));
    assert!(rects.is_empty() && swaps.is_empty(), "a fresh field baked");

    let mut field = WetField::new(device, queue, width, height, settings).expect("a wet field");
    field.inject(device, queue, (32, 32), 20.0, 1.0, [0.9, 0.1, 0.1]);
    field.advance(device, queue, 180);
    assert!(field.is_dry(device, queue), "the field did not dry");

    let first = field.deposit(device, queue).expect("a deposit");
    let (_, swaps) = bake(&mut stack, &renderer, 0, &first, (0, 0, width, height));
    assert!(!swaps.is_empty());
    let once = stack
        .read_layer_rect(device, queue, 0, (0, 0, width, height))
        .expect("a read-back tile");
    assert!(
        once.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
        "nothing baked"
    );

    let second = field.deposit(device, queue).expect("a deposit");
    bake(&mut stack, &renderer, 0, &second, (0, 0, width, height));
    let twice = stack
        .read_layer_rect(device, queue, 0, (0, 0, width, height))
        .expect("a read-back tile");
    assert_eq!(once.pixels, twice.pixels, "the dry bake darkened the layer");
}

/// Only the wet rectangle the caller names is rewritten; a pixel in the same tile but outside the
/// deposit keeps its old colour.
#[test]
fn a_partially_wet_field_only_changes_the_wet_rect() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let canvas = (2048u32, 2048u32);
    let settings = WetSettings {
        absorption: 0.5,
        ..WetSettings::default()
    };
    let mut field =
        WetField::for_canvas(device, queue, canvas, 0.5, settings).expect("a wet field");
    field.inject(device, queue, (1500, 1500), 128.0, 1.0, [1.0, 0.0, 0.0]);
    field.advance(device, queue, 60);
    let deposit = field.deposit(device, queue).expect("a deposit");

    let mut stack = LayerStack::new(canvas.0, canvas.1);
    let mut pixels = vec![0u8; (canvas.0 * canvas.1 * 4) as usize];
    // One blue pixel in the wet tile but outside the request, one in another tile.
    let mark = |pixels: &mut [u8], x: u32, y: u32| {
        let at = ((y * canvas.0 + x) * 4) as usize;
        pixels[at + 2] = 255;
        pixels[at + 3] = 255;
    };
    mark(&mut pixels, 1100, 1100);
    mark(&mut pixels, 100, 100);
    assert!(stack.set(device, queue, 0, &pixels));
    stack.dirty_tiles.clear();

    let rect = (1350, 1350, 300, 300);
    let (rects, swaps) = bake(&mut stack, &renderer, 0, &deposit, rect);
    assert_eq!(rects, vec![rect]);
    assert_eq!(swaps.len(), 1);
    assert_eq!(
        stack.dirty_tiles.iter().copied().collect::<Vec<_>>(),
        vec![(1, 1)],
        "a tile outside the request was marked dirty"
    );

    let out = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    assert_eq!(pixel(&out, canvas.0, 100, 100), [0, 0, 255, 255]);
    assert_eq!(pixel(&out, canvas.0, 1100, 1100), [0, 0, 255, 255]);
    let wet = pixel(&out, canvas.0, 1500, 1500);
    assert!(
        wet[0] > 20 && wet[1] < 10,
        "the wet rect did not bake: {wet:?}"
    );
}

/// The deposit packing follows the documented straight approximation, and baking does not consume
/// the field: water and mobile pigment are still there afterwards.
#[test]
fn the_deposit_is_the_documented_straight_approximation() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let side = 64u32;
    let mut field =
        WetField::new(device, queue, side, side, WetSettings::default()).expect("a wet field");
    field.inject(device, queue, (32, 32), 16.0, 1.0, [0.2, 0.5, 0.1]);
    field.advance(device, queue, 30);
    let before = field.read(device, queue);
    let deposit = field.deposit(device, queue).expect("a deposit");

    let packed = read_frame(
        device,
        queue,
        deposit.texture(),
        side,
        side,
        FrameFormat::Rgba16,
    );
    let FramePixels::Linear(packed) = packed else {
        panic!("a half-float deposit");
    };
    let mobile = read_frame(
        device,
        queue,
        deposit.mobile_texture(),
        side,
        side,
        FrameFormat::Rgba16,
    );
    let FramePixels::Linear(mobile) = mobile else {
        panic!("a half-float mobile texture");
    };

    let mut covered = 0;
    for index in 0..(side * side) as usize {
        let settled = [
            before[1][index][1],
            before[1][index][2],
            before[1][index][3],
        ];
        let density = settled.iter().copied().fold(0.0f32, f32::max);
        let baked = &packed[index * 4..index * 4 + 4];
        if density > 0.001 {
            covered += 1;
            let alpha = 1.0 - (-density).exp();
            assert!(
                (baked[3] - alpha).abs() < 0.01,
                "cell {index}: alpha {} vs {alpha}",
                baked[3]
            );
            for channel in 0..3 {
                assert!(
                    (baked[channel] - settled[channel] / density).abs() < 0.01,
                    "cell {index} channel {channel}: {} vs {}",
                    baked[channel],
                    settled[channel] / density
                );
            }
        }
        let suspended = [
            before[0][index][2],
            before[0][index][3],
            before[1][index][0],
        ];
        for channel in 0..3 {
            assert!(
                (mobile[index * 4 + channel] - suspended[channel]).abs() < 0.01,
                "mobile cell {index} channel {channel}"
            );
        }
    }
    assert!(covered > 0, "the disc settled no pigment");

    // The bake reads the field; it must not clear its water or mobile pigment.
    let mut stack = LayerStack::new(side, side);
    assert!(stack.set(device, queue, 0, &vec![0u8; (side * side * 4) as usize]));
    bake(&mut stack, &renderer, 0, &deposit, (0, 0, side, side));
    let after = field.read(device, queue);
    assert_eq!(before[0], after[0], "the bake changed water/pigment");
    assert_eq!(before[1], after[1], "the bake changed settled pigment");
}

/// The measured cost of one 512-pixel disc at 4K, printed with `cargo test ... -- --ignored
/// --nocapture`. It is a benchmark, not a correctness test, so it stays out of the default run.
#[test]
#[ignore = "benchmark: prints deposit + bake + journal time at 4K"]
fn a_wet_bake_at_4k_takes_a_bounded_time() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let canvas = (4096u32, 4096u32);
    let mut field = WetField::for_canvas(device, queue, canvas, 0.5, WetSettings::default())
        .expect("a wet field");
    field.inject(device, queue, (2048, 2048), 256.0, 1.0, [0.15, 0.3, 0.8]);
    field.advance(device, queue, 30);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    let mut stack = LayerStack::new(canvas.0, canvas.1);
    assert!(stack.set(
        device,
        queue,
        0,
        &vec![0u8; (canvas.0 * canvas.1 * 4) as usize]
    ));
    let _ = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);

    let started = Instant::now();
    let deposit = field.deposit(device, queue).expect("a deposit");
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let deposit_ms = started.elapsed().as_secs_f64() * 1e3;

    let rect = (1792, 1792, 512, 512);
    // The surface owns the pipeline; build it outside the measured bake.
    let pipeline = LayerStack::wet_pipeline(device, stack.format().texture());
    let run = |stack: &mut LayerStack| -> (f64, usize, usize) {
        let started = Instant::now();
        let mut encoder = device.create_command_encoder(&Default::default());
        let (rects, swaps) =
            stack.commit_wet(device, queue, &mut encoder, &pipeline, 0, &deposit, rect);
        queue.submit(Some(encoder.finish()));
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        (
            started.elapsed().as_secs_f64() * 1e3,
            rects.len(),
            swaps.len(),
        )
    };
    // One cold bake, then three warm ones: texture allocation and first-use driver work are real,
    // but a single sample of them is too noisy to report.
    let (cold_ms, _, _) = run(&mut stack);
    let mut samples = Vec::with_capacity(3);
    let (mut tiles, mut replaced) = (0, 0);
    for _ in 0..3 {
        let (ms, touched, swaps) = run(&mut stack);
        samples.push(ms);
        tiles = touched;
        replaced = swaps;
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let bake_ms = samples[1];
    assert!(tiles > 0 && tiles == replaced);

    let started = Instant::now();
    let _ = renderer.composite_stack(&mut stack, &[LayerSpec::new(0)]);
    let composite_ms = started.elapsed().as_secs_f64() * 1e3;

    let journal = replaced * stack.tile_bytes_hint();
    println!(
        "wet bake 4096x4096 field 2048x2048, 512px disc: deposit {deposit_ms:.2} ms, \
         bake+journal cold {cold_ms:.2} ms, warm median {bake_ms:.2} ms \
         ({tiles} tiles, {:.0} MiB), composite+readback {composite_ms:.2} ms",
        journal as f64 / (1u64 << 20) as f64
    );
}
