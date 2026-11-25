use crate::Renderer;
use drawkit_figure::{Camera, Figure, Preset};

#[test]
fn a_mannequin_reaches_the_gpu_target() {
    let Some(renderer) = Renderer::new() else {
        // A CI machine without a GPU is not a failing renderer.
        return;
    };
    let figure = Figure::from_preset(Preset::RealisticNeutral);
    let camera = Camera::orbit(glam::Vec3::new(0.0, 0.9, 0.0), 4.5, 0.2, 0.1);
    let pixels = renderer.render(&figure.mesh(), &camera, 96, 144, [0.7, 0.7, 0.7]);
    assert_eq!(pixels.len(), 96 * 144 * 4);
    let gpu_pixels = pixels.as_chunks::<4>().0;
    assert!(
        gpu_pixels.iter().any(|pixel| pixel[3] == 255),
        "the GPU drew nothing"
    );

    // The CPU rasteriser is the reference the GPU has to agree with. Edge pixels may land on
    // either side of the coverage threshold; the silhouette as a whole may not.
    let cpu = figure.render(&camera, 96, 144, [0.7, 0.7, 0.7]);
    let mut agreed = 0;
    let mut covered = 0;
    for (cpu, gpu) in cpu.iter().zip(gpu_pixels.iter()) {
        let cpu_covers = cpu[3] > 0.5;
        let gpu_covers = gpu[3] > 127;
        if cpu_covers || gpu_covers {
            covered += 1;
            agreed += usize::from(cpu_covers == gpu_covers);
        }
    }
    let ratio = agreed as f32 / covered as f32;
    assert!(
        ratio > 0.95,
        "GPU and CPU disagree on {:.1}% of the silhouette",
        (1.0 - ratio) * 100.0
    );
}
