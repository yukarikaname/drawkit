//! Stamp geometry and the small deterministic hashes the brush uses for jitter and grain.
//!
//! A round brush only needs a distance, but a bristle or a spray also needs an ellipse, a random
//! offset per stamp and a deterministic grain. Everything here is derived from the stroke seed, the
//! segment endpoints and the stamp's ordinal within the segment, so a replay paints the same
//! texture without storing per-pixel noise - and `stroke.wgsl` evaluates the same hashes, so both
//! paths paint alike.

pub(super) fn coverage(distance: f32, radius: f32, hardness: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    let t = (1.0 - distance / radius).clamp(0.0, 1.0);
    // Keep a sliver of falloff even for the hardest brush, or the rim turns into a staircase.
    let edge = (1.0 - hardness).max(0.04);
    if t >= edge {
        1.0
    } else {
        let u = t / edge;
        u * u * (3.0 - 2.0 * u)
    }
}

/// Distance from the stamp centre after rotating by `angle` and squeezing the short axis by
/// `roundness`. A roundness of one and any angle is a circle.
pub(super) fn shaped_distance(dx: f32, dy: f32, angle: f32, roundness: f32) -> f32 {
    let (sin, cos) = angle.sin_cos();
    let along = dx * cos + dy * sin;
    let across = -dx * sin + dy * cos;
    let roundness = roundness.clamp(0.05, 1.0);
    (along * along + (across / roundness) * (across / roundness)).sqrt()
}

/// The lattice hash shared with `stroke.wgsl` (`mix32`).
pub(super) fn mix32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value
}

/// The integer lattice hash shared with `stroke.wgsl` (`noise`), for the per-pixel grain.
///
/// Wrapping 32-bit arithmetic, so the CPU and the GPU land on the same value for the same pixel,
/// seed half and salt. Changing a constant here changes every stroke's texture.
pub(super) fn noise(x: u32, y: u32, salt: u32, seed: u64) -> f32 {
    let mut value = (x ^ seed as u32).wrapping_mul(0x85EB_CA77)
        ^ (y ^ (seed >> 32) as u32).wrapping_mul(0x9E37_79B1)
        ^ salt;
    value = mix32(value);
    (value & 0xFFFF) as f32 / 65535.0
}

/// Mix the stroke seed with the segment endpoints, so successive segments do not repeat the same
/// jitter stream while a replay of the same segment still does. `stroke.wgsl` has the same function.
pub(super) fn segment_hash(seed: u64, from: (f32, f32), to: (f32, f32)) -> u32 {
    let mut value = seed as u32 ^ mix32((seed >> 32) as u32 ^ 0x9E37_79B9);
    value = mix32(value ^ from.0.to_bits());
    value = mix32(value ^ from.1.to_bits());
    value = mix32(value ^ to.0.to_bits());
    mix32(value ^ to.1.to_bits())
}

fn stream_value(segment: u32, ordinal: u32, salt: u32) -> f32 {
    let value = mix32(segment ^ ordinal.wrapping_mul(0x9E37_79B1) ^ salt);
    (value & 0xFFFF) as f32 / 65535.0
}

/// The offset `stroke.wgsl`'s `jittered` adds to a stamp centre.
///
/// The stamp's ordinal within the segment and the segment hash pick the offset, so the CPU and the
/// GPU agree bit for bit without depending on the exact float position of the stamp. A replay of
/// the recorded segments reproduces the stroke; splitting one stroke into different segments
/// gives the pieces a different stream, exactly like the recorded stroke would.
pub(super) fn jitter(
    segment: u32,
    ordinal: u32,
    amount: f32,
    radius: f32,
    salt: u32,
) -> (f32, f32) {
    if amount <= 0.0 {
        return (0.0, 0.0);
    }
    (
        (stream_value(segment, ordinal, salt) * 2.0 - 1.0) * radius * amount,
        (stream_value(segment, ordinal, salt.wrapping_add(1)) * 2.0 - 1.0) * radius * amount,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundness_only_shrinks_the_short_axis() {
        let quarter = std::f32::consts::FRAC_PI_2;
        assert!((shaped_distance(5.0, 0.0, 0.0, 1.0) - 5.0).abs() < 1e-5);
        assert!((shaped_distance(0.0, 5.0, 0.0, 0.5) - 10.0).abs() < 1e-5);
        assert!((shaped_distance(0.0, 5.0, quarter, 0.5) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn the_noise_is_repeatable_and_seed_dependent() {
        let a = [
            noise(13, 40, 0x51ED_2701, 42),
            noise(14, 40, 0x51ED_2701, 42),
        ];
        assert_eq!(
            a,
            [
                noise(13, 40, 0x51ED_2701, 42),
                noise(14, 40, 0x51ED_2701, 42)
            ]
        );
        assert_ne!(a[0], noise(13, 40, 0x51ED_2701, 43));
        assert!(a.iter().all(|value| (0.0..=1.0).contains(value)));
    }

    #[test]
    fn jitter_is_stable_per_segment_and_ordinal() {
        let segment = segment_hash(7, (12.4, 40.9), (30.0, 41.0));
        let first = jitter(segment, 0, 0.5, 8.0, 0x9E37_79B1);
        let same = jitter(segment, 0, 0.5, 8.0, 0x9E37_79B1);
        let next = jitter(segment, 1, 0.5, 8.0, 0x9E37_79B1);
        let other_segment = segment_hash(7, (12.4, 40.9), (30.0, 41.5));
        let other = jitter(other_segment, 0, 0.5, 8.0, 0x9E37_79B1);
        assert_eq!(first, same);
        assert_ne!(first, next);
        assert_ne!(first, other);
        assert!(first.0.abs() <= 4.0 && first.1.abs() <= 4.0);
        assert_eq!(jitter(segment, 0, 0.0, 8.0, 1), (0.0, 0.0));
    }
}
