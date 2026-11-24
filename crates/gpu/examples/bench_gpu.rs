// CPU and GPU timings for the image operations, at 1080p and 4K.
//
//   cargo run --release -p drawkit-gpu --example bench_gpu

use drawkit_core::color::Rgba;
use drawkit_core::fill::{Mask, erode, flood, grow};
use drawkit_gpu::{Layer, Morphology, Renderer};
use std::time::Instant;

fn timed<T>(mut run: impl FnMut() -> T, rounds: usize) -> std::time::Duration {
    let started = Instant::now();
    for _ in 0..rounds {
        std::hint::black_box(run());
    }
    started.elapsed() / rounds as u32
}

fn main() {
    let Some(renderer) = Renderer::new() else {
        println!("no wgpu adapter; GPU timings unavailable");
        return;
    };
    for (width, height) in [(1920usize, 1080usize), (3840, 2160)] {
        println!("--- {width}x{height}");
        let mask: Vec<f32> = (0..width * height)
            .map(|index| {
                let (x, y) = (index % width, index / width);
                f32::from(
                    x > width / 4 && x < width * 3 / 4 && y > height / 3 && y < height * 2 / 3,
                )
            })
            .collect();

        let cpu = timed(
            || {
                grow(
                    &Mask {
                        width,
                        height,
                        data: mask.clone(),
                    },
                    4,
                )
            },
            2,
        );
        let gpu = timed(
            || renderer.morphology(&mask, width as u32, height as u32, 4, Morphology::Grow),
            4,
        );
        println!("grow r=4    CPU {cpu:>10.2?}   GPU {gpu:>10.2?}");

        let cpu = timed(
            || {
                grow(
                    &Mask {
                        width,
                        height,
                        data: mask.clone(),
                    },
                    32,
                )
            },
            1,
        );
        let gpu = timed(
            || renderer.morphology(&mask, width as u32, height as u32, 32, Morphology::Grow),
            2,
        );
        println!("grow r=32   CPU {cpu:>10.2?}   GPU {gpu:>10.2?}");

        let buffers: Vec<Vec<u8>> = (0..4)
            .map(|layer| {
                let mut bytes = Vec::with_capacity(width * height * 4);
                for index in 0..width * height {
                    let shade = ((index % 255) as u8).wrapping_add(layer as u8 * 40);
                    bytes.extend_from_slice(&[shade, shade / 2, 255 - shade, 128]);
                }
                bytes
            })
            .collect();
        let layers: Vec<Layer<'_>> = buffers.iter().map(|bytes| Layer::new(bytes, 0.8)).collect();
        let cpu_pixels = vec![[0.0f32; 4]; width * height];
        let cpu = timed(
            || {
                let mut out = cpu_pixels.clone();
                for (index, slot) in out.iter_mut().enumerate() {
                    for bytes in &buffers {
                        let alpha = bytes[index * 4 + 3] as f32 / 255.0 * 0.8;
                        *slot = [
                            slot[0] * (1.0 - alpha) + bytes[index * 4] as f32 / 255.0 * alpha,
                            slot[1] * (1.0 - alpha) + bytes[index * 4 + 1] as f32 / 255.0 * alpha,
                            slot[2] * (1.0 - alpha) + bytes[index * 4 + 2] as f32 / 255.0 * alpha,
                            slot[3] * (1.0 - alpha) + alpha,
                        ];
                    }
                }
                out
            },
            1,
        );
        let gpu = timed(
            || renderer.composite(&layers, width as u32, height as u32),
            2,
        );
        println!("composite 4 CPU {cpu:>10.2?}   GPU {gpu:>10.2?} (uploads each call)");

        let mut stack = renderer.layer_stack(width as u32, height as u32);
        for (index, bytes) in buffers.iter().enumerate() {
            stack.set(renderer.device(), renderer.queue(), index, bytes);
        }
        let specs: Vec<drawkit_gpu::LayerSpec> = (0..buffers.len())
            .map(drawkit_gpu::LayerSpec::new)
            .collect();
        let resident = timed(
            || {
                let texture = renderer.composite_stack_texture(&stack, &specs);
                let _ = renderer.device().poll(wgpu::PollType::wait_indefinitely());
                texture.is_some()
            },
            4,
        );
        println!("composite 4 GPU resident {resident:>10.2?} (no readback)");

        let image: Vec<Rgba> = (0..width * height)
            .map(|index| {
                let (x, y) = (index % width, index / width);
                if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
                    [0.0, 0.0, 0.0, 1.0]
                } else {
                    [1.0, 1.0, 1.0, 1.0]
                }
            })
            .collect();
        let fill = timed(
            || flood(&image, width, height, (width / 2, height / 2), 0.1),
            1,
        );
        println!("flood 4K    CPU {fill:>10.2?}");

        let eroded = timed(
            || {
                erode(
                    &Mask {
                        width,
                        height,
                        data: mask.clone(),
                    },
                    4,
                )
            },
            1,
        );
        println!("erode r=4   CPU {eroded:>10.2?}");
    }
}
