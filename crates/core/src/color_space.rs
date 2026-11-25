//! The working colour space and light range of a document.
//!
//! Pixels are stored in the space's encoded coordinates. Everything that needs linear light - the
//! extended-range display, the EXR export - decodes here and converts into linear Display P3, the
//! one wide extended space a `CAMetalLayer` takes directly.

/// The working RGB space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace {
    Srgb,
    DisplayP3,
    AdobeRgb,
}

impl ColorSpace {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::DisplayP3,
            2 => Self::AdobeRgb,
            _ => Self::Srgb,
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::Srgb => 0,
            Self::DisplayP3 => 1,
            Self::AdobeRgb => 2,
        }
    }

    /// Decode a stored channel into linear light. Above one the curve keeps going, so an HDR
    /// canvas can store a highlight brighter than paper.
    pub fn decode(self, value: f32) -> f32 {
        match self {
            Self::Srgb | Self::DisplayP3 => crate::color::decode_srgb(value),
            Self::AdobeRgb => crate::color::decode_gamma(value),
        }
    }

    pub fn encode(self, linear: f32) -> f32 {
        match self {
            Self::Srgb | Self::DisplayP3 => crate::color::encode_srgb(linear),
            Self::AdobeRgb => crate::color::encode_gamma(linear),
        }
    }

    /// Linear Display P3 back to linear working RGB.
    pub fn from_display_p3(self) -> [[f32; 3]; 3] {
        inverse(self.to_display_p3())
    }

    /// Linear working RGB to linear Display P3.
    pub fn to_display_p3(self) -> [[f32; 3]; 3] {
        match self {
            Self::Srgb => [
                [0.822_462_1, 0.177_537_9, 0.0],
                [0.033_194_1, 0.966_805_9, 0.0],
                [0.017_082_6, 0.072_397_4, 0.910_519_9],
            ],
            Self::DisplayP3 => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            Self::AdobeRgb => [
                [1.150_217_9, -0.150_090_9, 0.0],
                [0.046_422_2, 0.953_560_7, 0.0],
                [0.023_890_2, 0.026_504_1, 0.949_388_2],
            ],
        }
    }
}

fn inverse(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let a = m[0][0];
    let b = m[0][1];
    let c = m[0][2];
    let d = m[1][0];
    let e = m[1][1];
    let f = m[1][2];
    let g = m[2][0];
    let h = m[2][1];
    let i = m[2][2];
    let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if determinant.abs() < 1e-12 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let scale = 1.0 / determinant;
    [
        [
            (e * i - f * h) * scale,
            (c * h - b * i) * scale,
            (b * f - c * e) * scale,
        ],
        [
            (f * g - d * i) * scale,
            (a * i - c * g) * scale,
            (c * d - a * f) * scale,
        ],
        [
            (d * h - e * g) * scale,
            (b * g - a * h) * scale,
            (a * e - b * d) * scale,
        ],
    ]
}

/// Whether the canvas may hold light above paper white. The pixels are floats either way; the
/// flag is what tells the shell to present through an extended-range surface and to export EXR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicRange {
    Sdr,
    Hdr,
}

impl DynamicRange {
    pub fn from_i32(value: i32) -> Self {
        if value == 1 { Self::Hdr } else { Self::Sdr }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::Sdr => 0,
            Self::Hdr => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_maps_to_white_in_every_space() {
        for space in [
            ColorSpace::Srgb,
            ColorSpace::DisplayP3,
            ColorSpace::AdobeRgb,
        ] {
            let linear = space.decode(1.0);
            assert!((linear - 1.0).abs() < 0.01, "{space:?} decoded to {linear}");
            let matrix = space.to_display_p3();
            for row in matrix {
                assert!((row.iter().sum::<f32>() - 1.0).abs() < 0.01, "{row:?}");
            }
        }
    }

    #[test]
    fn the_display_p3_matrix_has_an_inverse() {
        for space in [
            ColorSpace::Srgb,
            ColorSpace::DisplayP3,
            ColorSpace::AdobeRgb,
        ] {
            let forward = space.to_display_p3();
            let back = space.from_display_p3();
            let columns: [[f32; 3]; 3] =
                std::array::from_fn(|column| std::array::from_fn(|k| back[k][column]));
            for (row, forward_row) in forward.iter().enumerate() {
                for (column, back_column) in columns.iter().enumerate() {
                    let value: f32 = forward_row
                        .iter()
                        .zip(back_column)
                        .map(|(a, b)| a * b)
                        .sum();
                    let expected = if row == column { 1.0 } else { 0.0 };
                    assert!(
                        (value - expected).abs() < 1e-4,
                        "{space:?} [{row}][{column}] = {value}"
                    );
                }
            }
        }
    }
}
