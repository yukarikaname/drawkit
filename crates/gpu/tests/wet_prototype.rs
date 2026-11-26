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
