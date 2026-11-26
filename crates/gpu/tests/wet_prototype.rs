//! The first wet-media experiment: blue and red on wet paper, stepping until they meet and dry.
//! This is a plausibility check, not a reference render.

use drawkit_gpu::{Renderer, WetField, WetSettings};

#[test]
fn wet_blue_and_red_meet_and_settle() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let side = 256u32;
    let mut field =
        WetField::new(device, queue, side, side, WetSettings::default()).expect("a wet field");
    field.inject(device, queue, (96, 128), 24.0, 1.0, [0.0, 0.0, 1.0]);
    field.inject(device, queue, (160, 128), 24.0, 1.0, [1.0, 0.0, 0.0]);

    let started = std::time::Instant::now();
    field.advance(device, queue, 900);
    let each = started.elapsed().as_secs_f64() * 1000.0 / 900.0;

    let [state0, state1] = field.read(device, queue);
    let at = |x: u32, y: u32| {
        let index = (y * side + x) as usize;
        (state0[index], state1[index])
    };
    let (_, blue) = at(96, 128);
    assert!(blue[0] > 0.01, "blue at its centre: {blue:?}");
    let (red, _) = at(160, 128);
    assert!(red[2] > 0.01, "red at its centre: {red:?}");

    let (middle0, middle1) = at(128, 128);
    assert!(
        middle0[2] > 0.0002 && middle1[0] > 0.0002,
        "blue and red met: red {:?} blue {:?}",
        middle0,
        middle1
    );

    let deposited: f32 = state1.iter().map(|p| p[1] + p[2] + p[3]).sum();
    assert!(deposited > 0.01, "pigment settled: {deposited}");

    let (centre0, _) = at(96, 128);
    assert!(
        centre0[0] < 0.99,
        "water evaporated at the centre: {centre0:?}"
    );

    for pixel in state0.iter().chain(state1.iter()) {
        for value in pixel {
            assert!(*value >= 0.0 && value.is_finite(), "bad cell {pixel:?}");
        }
    }
    println!(
        "wet 256x256: {each:.3} ms/step, deposited {deposited:.3}, midpoint red {:.4} blue {:.4}",
        middle0[2], middle1[0]
    );
}

/// What a document-sized field costs, and what one stroke of injection costs, at the scales the
/// runtime pins for large canvases.
#[test]
#[ignore]
fn a_wet_field_step_cost_at_document_sizes() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    for (canvas, scale) in [
        ((1920, 1080), 1.0),
        ((3840, 2160), 0.5),
        ((7680, 4320), 0.25),
    ] {
        let mut field =
            WetField::for_canvas(device, queue, canvas, scale, WetSettings::default())
                .expect("a wet field");
        let (width, height) = field.size();
        let strokes = 50;
        let started = std::time::Instant::now();
        for index in 0..strokes {
            let y = height as f32 * (index as f32 + 0.5) / strokes as f32;
            field.inject_stroke(
                device,
                queue,
                (64.0, y),
                (width as f32 - 64.0, y),
                32.0,
                1.0,
                [0.1, 0.2, 0.8],
            );
        }
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let inject = started.elapsed().as_secs_f64() * 1000.0 / strokes as f64;

        // A real pointer event covers tens of pixels, not the whole row: three to five stamps.
        let events = 20;
        let started = std::time::Instant::now();
        for _ in 0..events {
            field.inject_stroke(
                device,
                queue,
                (128.0, 128.0),
                (188.0, 140.0),
                32.0,
                1.0,
                [0.1, 0.2, 0.8],
            );
        }
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let event = started.elapsed().as_secs_f64() * 1000.0 / events as f64;

        // Every cell is wet after those strokes, so this is the worst case for the step.
        let steps = 60;
        let started = std::time::Instant::now();
        field.advance(device, queue, steps);
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let each = started.elapsed().as_secs_f64() * 1000.0 / steps as f64;
        let vram = 4.0 * width as f64 * height as f64 * 8.0 / (1u64 << 20) as f64;
        println!(
            "wet {}x{} at {scale}: field {width}x{height}, {vram:.0} MiB, \
             inject {inject:.3} ms/row, {event:.3} ms/event, step {each:.3} ms",
            canvas.0, canvas.1
        );
    }
}

#[test]
fn a_half_resolution_field_covers_a_4k_canvas() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let canvas = (3840, 2160);
    let mut field =
        WetField::for_canvas(device, queue, canvas, 0.5, WetSettings::default())
            .expect("a wet field");
    assert_eq!(field.canvas_size(), canvas);
    assert_eq!(field.size(), (1920, 1080));
    assert!((field.scale() - 0.5).abs() < f32::EPSILON);

    field.inject_stroke(
        device,
        queue,
        (100.0, 100.0),
        (3700.0, 2000.0),
        24.0,
        1.0,
        [0.2, 0.3, 0.4],
    );
    let [state0, _] = field.read(device, queue);
    let at = |x: u32, y: u32| state0[(y * field.size().0 + x) as usize];
    // The document midpoint is (1920, 1080), field (960, 540).
    let (x, y) = (960, 540);
    assert!(at(x, y)[0] > 0.1, "water at the midpoint: {:?}", at(x, y));
}

#[test]
fn a_stroke_leaves_a_wet_trail() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let side = 256u32;
    let mut field =
        WetField::new(device, queue, side, side, WetSettings::default()).expect("a wet field");
    field.inject_stroke(
        device,
        queue,
        (40.0, 128.0),
        (216.0, 128.0),
        8.0,
        1.0,
        [0.0, 0.0, 1.0],
    );
    let [state0, state1] = field.read(device, queue);
    for x in [40u32, 80, 120, 160, 200, 216] {
        let water = state0[(128 * side + x) as usize][0];
        assert!(water > 0.5, "water at x {x}: {water}");
    }
    // Blue pigment sits in the first channel of the second half.
    let pigment = state1[(128 * side + 128) as usize][0];
    assert!(pigment > 0.5, "pigment on the trail: {pigment}");

    field.advance(device, queue, 120);
    let [state0, state1] = field.read(device, queue);
    let deposited: f32 = state1.iter().map(|p| p[1] + p[2] + p[3]).sum();
    assert!(deposited > 0.01, "pigment settled: {deposited}");
    let _ = state0;
}

#[test]
fn a_field_reports_when_it_is_dry() {
    let Some(renderer) = Renderer::new() else {
        return;
    };
    let (device, queue) = (renderer.device(), renderer.queue());
    let settings = WetSettings {
        evaporation: 2.0,
        ..WetSettings::default()
    };
    let mut field =
        WetField::new(device, queue, 128, 128, settings).expect("a wet field");
    field.inject(device, queue, (64, 64), 20.0, 1.0, [0.9, 0.1, 0.1]);
    assert!(!field.is_dry(device, queue), "a fresh disc is wet");
    field.advance(device, queue, 60);
    assert!(field.is_dry(device, queue), "the water evaporated");
}
