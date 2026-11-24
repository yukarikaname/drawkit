// Tiny shape model latency: cargo run --release -p drawkit-core --example bench_shape
//
// A finished stroke calls this once. The number that matters is per call at the pen-up boundary,
// not throughput.

use drawkit_core::shape::classify;
use std::time::Instant;

fn main() {
    let points: Vec<(f32, f32)> = (0..32)
        .map(|i| {
            (
                i as f32 * 5.0,
                (i as f32 * 0.4).sin() * 3.0 + i as f32 * 0.2,
            )
        })
        .collect();
    let rounds = 100_000;
    let started = Instant::now();
    let mut total = 0.0;
    for _ in 0..rounds {
        total += classify(&points).confidence;
    }
    let elapsed = started.elapsed();
    println!(
        "{rounds} classifications: {:?} ({:.3} µs each, checksum {:.1})",
        elapsed,
        elapsed.as_secs_f64() * 1_000_000.0 / rounds as f64,
        total
    );
}
