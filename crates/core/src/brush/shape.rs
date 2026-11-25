//! Stamp geometry and the small deterministic stream the brush uses for jitter and grain.
//!
//! A round brush only needs a distance, but a bristle or a spray also needs an ellipse, a random
//! offset per stamp and a deterministic grain. Everything here is derived from the stroke seed and
//! the segment endpoints, so a replay paints the same texture without storing per-pixel noise.

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

pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// Uniform in `[0, 1)`.
    pub(super) fn next(&mut self) -> f32 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        (value >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Mix the stroke seed with the segment so successive segments do not repeat the same jitter
/// pattern while a replay of the same segment still does.
pub(super) fn segment_seed(seed: u64, from: (f32, f32), to: (f32, f32)) -> u64 {
    let mut hash = seed ^ 0x9E37_79B9_7F4A_7C15;
    for value in [from.0, from.1, to.0, to.1] {
        hash ^= value.to_bits() as u64;
        hash = hash.wrapping_mul(0x100_0000_01B3);
        hash ^= hash >> 29;
    }
    hash | 1
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
    fn the_grain_stream_is_repeatable_and_moves() {
        let mut first = Rng::new(segment_seed(42, (1.0, 2.0), (3.0, 4.0)));
        let mut second = Rng::new(segment_seed(42, (1.0, 2.0), (3.0, 4.0)));
        let mut other = Rng::new(segment_seed(42, (1.0, 2.0), (3.0, 5.0)));
        let a = [first.next(), first.next(), first.next()];
        let b = [second.next(), second.next(), second.next()];
        let c = [other.next(), other.next(), other.next()];
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.iter().all(|value| (0.0..1.0).contains(value)));
    }
}
