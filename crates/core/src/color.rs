//! Colours, and the two operations a paint app gets wrong at the edges.
//!
//! Everything here is straight (non-premultiplied) RGBA in 0..1. Premultiplied storage is what a
//! compositor wants; straight is what a colour picker and a brush want. The conversion is one
//! multiply, and doing it in the wrong place is what leaves a white rim around a filled shape.

pub type Rgba = [f32; 4];

const EPSILON: f32 = 1.0 / 512.0;

/// Decode one sRGB channel into linear light. Above one the curve keeps going, so an HDR canvas
/// can store a highlight that is brighter than paper.
pub fn decode_srgb(value: f32) -> f32 {
    let value = value.max(0.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub fn encode_srgb(linear: f32) -> f32 {
    let linear = linear.max(0.0);
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// Adobe RGB (1998) uses a plain 2.2 gamma rather than the sRGB piecewise curve.
pub fn decode_gamma(value: f32) -> f32 {
    value.max(0.0).powf(2.199_218_8)
}

pub fn encode_gamma(linear: f32) -> f32 {
    linear.max(0.0).powf(1.0 / 2.199_218_8)
}

pub fn premultiply(c: Rgba) -> Rgba {
    [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]]
}

pub fn unpremultiply(c: Rgba) -> Rgba {
    if c[3] <= EPSILON {
        return [0.0, 0.0, 0.0, 0.0];
    }
    [
        (c[0] / c[3]).min(1.0),
        (c[1] / c[3]).min(1.0),
        (c[2] / c[3]).min(1.0),
        c[3],
    ]
}

/// The colour-wheel relationships the palette offers. Each kind lists hue rotations in degrees;
/// the base colour is not repeated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarmonyKind {
    Complementary,
    Analogous,
    SplitComplementary,
    Triadic,
    Tetradic,
}

impl HarmonyKind {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Analogous,
            2 => Self::SplitComplementary,
            3 => Self::Triadic,
            4 => Self::Tetradic,
            _ => Self::Complementary,
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::Complementary => 0,
            Self::Analogous => 1,
            Self::SplitComplementary => 2,
            Self::Triadic => 3,
            Self::Tetradic => 4,
        }
    }

    fn offsets(self) -> &'static [f32] {
        match self {
            Self::Complementary => &[180.0],
            Self::Analogous => &[-30.0, 30.0],
            Self::SplitComplementary => &[150.0, 210.0],
            Self::Triadic => &[120.0, 240.0],
            Self::Tetradic => &[90.0, 180.0, 270.0],
        }
    }
}

/// The other colours on the wheel for `base`. Hue rotates; saturation, lightness and alpha stay.
pub fn harmony(base: Rgba, kind: HarmonyKind) -> Vec<Rgba> {
    let (hue, saturation, lightness) = rgb_to_hsl([base[0], base[1], base[2]]);
    kind.offsets()
        .iter()
        .map(|offset| {
            let rotated = (hue + offset / 360.0).rem_euclid(1.0);
            let [red, green, blue] = hsl_to_rgb(rotated, saturation, lightness);
            [red, green, blue, base[3]]
        })
        .collect()
}

fn rgb_to_hsl(rgb: [f32; 3]) -> (f32, f32, f32) {
    let [red, green, blue] = rgb.map(|channel| channel.clamp(0.0, 1.0));
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (max + min) * 0.5;
    let delta = max - min;
    if delta <= 1e-6 {
        return (0.0, 0.0, lightness);
    }
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs()).max(1e-6);
    let hue = if max == red {
        ((green - blue) / delta).rem_euclid(6.0)
    } else if max == green {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    } / 6.0;
    (hue, saturation, lightness)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let hue = hue.rem_euclid(1.0);
    let saturation = saturation.clamp(0.0, 1.0);
    let lightness = lightness.clamp(0.0, 1.0);
    if saturation <= 1e-6 {
        return [lightness; 3];
    }
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue * 6.0;
    let x = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let [red, green, blue] = match sector as u32 {
        0 => [chroma, x, 0.0],
        1 => [x, chroma, 0.0],
        2 => [0.0, chroma, x],
        3 => [0.0, x, chroma],
        4 => [x, 0.0, chroma],
        _ => [chroma, 0.0, x],
    };
    let base = lightness - chroma * 0.5;
    [red + base, green + base, blue + base]
}

/// `top` over `bottom`, straight alpha in and out.
pub fn over(top: Rgba, bottom: Rgba) -> Rgba {
    let a = top[3] + bottom[3] * (1.0 - top[3]);
    if a <= EPSILON {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let mix = |t: f32, b: f32| (t * top[3] + b * bottom[3] * (1.0 - top[3])) / a;
    [
        mix(top[0], bottom[0]),
        mix(top[1], bottom[1]),
        mix(top[2], bottom[2]),
        a,
    ]
}

/// The colour a pixel would have had if `background` had not been behind it.
///
/// A pixel on an anti-aliased edge of an opaque composite is
/// `a * foreground + (1 - a) * background`, and the layer's own alpha is not in the pixel - it is in
/// the selection, the mask, or the shape that made it. `alpha` is that number, and solving the
/// equation for the foreground is the whole fix:
///
/// ```text
/// foreground = (pixel - (1 - a) * background) / a
/// ```
///
/// The app calls this "colour decontamination", and it is the difference between a cut-out that
/// sits on a new background and one with a white halo.
pub fn decontaminate(pixel: Rgba, background: Rgba, alpha: f32) -> Rgba {
    let a = alpha.clamp(0.0, 1.0);
    if a <= EPSILON {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let solve = |p: f32, b: f32| ((p - (1.0 - a) * b) / a).clamp(0.0, 1.0);
    [
        solve(pixel[0], background[0]),
        solve(pixel[1], background[1]),
        solve(pixel[2], background[2]),
        a,
    ]
}

/// How much of `pixel` is the painted colour rather than the paper behind it, in 0..1.
///
/// This is the anti-aliasing coverage, recovered by projecting the pixel onto the line from the
/// background to the paint. A flood fill that writes a hard 0 or 1 leaves the edge pixels untouched
/// and the paper shows through them - the white fringe. Writing this fraction instead is what makes
/// a fill reach the edge without a rim.
pub fn coverage(pixel: Rgba, background: Rgba, paint: Rgba) -> f32 {
    let dir = [
        paint[0] - background[0],
        paint[1] - background[1],
        paint[2] - background[2],
    ];
    let len = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
    if len <= EPSILON * EPSILON {
        return pixel[3];
    }
    let delta = [
        pixel[0] - background[0],
        pixel[1] - background[1],
        pixel[2] - background[2],
    ];
    ((delta[0] * dir[0] + delta[1] * dir[1] + delta[2] * dir[2]) / len).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_round_trips() {
        let c = [0.2, 0.5, 0.9, 0.4];
        let back = unpremultiply(premultiply(c));
        for i in 0..4 {
            assert!((back[i] - c[i]).abs() < 1e-5, "{back:?} is not {c:?}");
        }
    }

    #[test]
    fn a_half_transparent_pixel_over_white_is_the_composite() {
        let fg = [1.0, 0.0, 0.0, 0.5];
        let bg = [1.0, 1.0, 1.0, 1.0];
        let out = over(fg, bg);
        // Half red over white: red stays one, green and blue are half way to zero.
        assert!((out[0] - 1.0).abs() < 1e-5);
        assert!((out[1] - 0.5).abs() < 1e-5 && (out[2] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn decontaminating_an_edge_recovers_the_paint() {
        // A pixel that is 80% red over white.
        let background = [1.0, 1.0, 1.0, 1.0];
        let paint = [0.9, 0.1, 0.1, 1.0];
        let edge = over([paint[0], paint[1], paint[2], 0.8], background);
        // The layer's alpha comes from the mask; the pixel is the flattened composite.
        let clean = decontaminate(edge, background, 0.8);
        for i in 0..3 {
            assert!(
                (clean[i] - paint[i]).abs() < 1e-4,
                "{clean:?} is not {paint:?}"
            );
        }
    }

    #[test]
    fn coverage_reads_the_antialiasing() {
        let background = [1.0, 1.0, 1.0, 1.0];
        let paint = [0.0, 0.0, 0.0, 1.0];
        let half = over([0.0, 0.0, 0.0, 0.5], background);
        assert!((coverage(half, background, paint) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn harmony_rotates_the_expected_hues() {
        let red = [1.0, 0.0, 0.0, 0.6];
        let complementary = harmony(red, HarmonyKind::Complementary);
        assert_eq!(complementary.len(), 1);
        assert!(complementary[0][1] > 0.9 && complementary[0][2] > 0.9);
        assert_eq!(complementary[0][3], 0.6);

        let triadic = harmony(red, HarmonyKind::Triadic);
        assert_eq!(triadic.len(), 2);
        assert!(triadic[0][1] > 0.9 && triadic[0][2] < 0.1);
        assert!(triadic[1][2] > 0.9 && triadic[1][1] < 0.1);
    }

    #[test]
    fn the_transfer_curves_round_trip_and_reach_past_white() {
        for value in [0.0, 0.02, 0.18, 0.5, 1.0, 1.5, 4.0] {
            let back = encode_srgb(decode_srgb(value));
            assert!((back - value).abs() < 1e-5, "{value} became {back}");
        }
        // Paper white decodes to one, and a value of two is a real highlight.
        assert!((decode_srgb(1.0) - 1.0).abs() < 1e-5);
        assert!((decode_srgb(2.0) - 4.95).abs() < 0.02);
        assert!(decode_gamma(1.0) > 0.99 && decode_gamma(1.0) < 1.01);
    }
}
