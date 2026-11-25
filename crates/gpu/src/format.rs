//! The two pixel formats the resident stacks use.

/// Eight-bit encoded pixels, or half-float linear Display P3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameFormat {
    Rgba8,
    Rgba16,
}

impl FrameFormat {
    pub const fn texture(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
            Self::Rgba16 => wgpu::TextureFormat::Rgba16Float,
        }
    }

    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba8 => 4,
            Self::Rgba16 => 8,
        }
    }

    pub fn from_texture(format: wgpu::TextureFormat) -> Option<Self> {
        match format {
            wgpu::TextureFormat::Rgba8Unorm => Some(Self::Rgba8),
            wgpu::TextureFormat::Rgba16Float => Some(Self::Rgba16),
            _ => None,
        }
    }
}
