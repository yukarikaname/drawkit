//! Wet media: water and pigment on paper, stepped as a fixed-dt field.
//!
//! One field pair holds eight channels in two `Rgba16Float` textures: water, wetness, mobile
//! pigment RGB and deposited RGB. Injection adds a stroke's water and pigment once per pointer
//! event; [`WetField::advance`] runs fixed 1/60 s steps. The step is gather-only, so a cell reads
//! the current pair and writes the next one, never a neighbour.
//!
//! The field is resident at a pinned simulation scale, not one texel per document pixel. A 4K
//! document at half resolution is 1920x1080, about 66 MB for both pairs, and a fully wet field
//! steps in 0.19 ms on the reference machine. Tiling the field would buy nothing at that size;
//! the reason to tile later is an 8K document whose field does not fit, not step cost.

mod field;

pub use field::WetField;

/// One texel is one f16x4; two textures make one buffer of the eight channels.
const STATE: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const WORKGROUP: u32 = 8;

/// The simulation clock. The shell passes elapsed seconds; the field steps in fixed increments so
/// a replay of the same input produces the same state regardless of frame rate.
pub const DT: f32 = 1.0 / 60.0;

/// Physical knobs. Constants for now; a material preset will own them.
#[derive(Clone, Copy, Debug)]
pub struct WetSettings {
    /// Water diffusion, in cells squared per second. Explicit Euler is stable while
    /// `diffusion * dt <= 0.25`.
    pub diffusion: f32,
    /// Water lost per second.
    pub evaporation: f32,
    /// Share of mobile pigment that settles per second where the paper is wet.
    pub absorption: f32,
    /// Extra deposition where water is receding, which is the dark rim.
    pub edge: f32,
    /// Pigment movement with the water gradient.
    pub advection: f32,
    /// Pigment diffusion while wet.
    pub pigment_diffusion: f32,
    /// Water below this is dry and the cell stops evolving.
    pub dry: f32,
}

impl Default for WetSettings {
    fn default() -> Self {
        Self {
            diffusion: 15.0,
            evaporation: 0.005,
            absorption: 0.03,
            edge: 1.5,
            advection: 2.0,
            pigment_diffusion: 6.0,
            dry: 0.001,
        }
    }
}

/// Field size for a document at `scale`, at least one cell. The scale is clamped so a shell can
/// pass a quality setting without producing an empty or larger-than-document field.
pub fn field_size(canvas: (u32, u32), scale: f32) -> (u32, u32) {
    let scale = scale.clamp(0.1, 1.0);
    let side = |value: u32| ((value as f32 * scale).round() as u32).max(1);
    (side(canvas.0), side(canvas.1))
}

/// Scalars only, so the Rust struct and the WGSL struct have the same layout. Injection writes
/// one of these per pointer event; `count` and the colour are ignored by the step entry.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    width: u32,
    height: u32,
    dt: f32,
    diffusion: f32,
    evaporation: f32,
    absorption: f32,
    edge: f32,
    advection: f32,
    pigment_diffusion: f32,
    dry: f32,
    count: u32,
    colour_r: f32,
    colour_g: f32,
    colour_b: f32,
    colour_a: f32,
}

/// The most stamps one pointer event can add. A segment longer than this is split by the brush
/// before it reaches the field.
pub(crate) const MAX_STAMPS: usize = 64;

impl Params {
    pub(crate) fn step(width: u32, height: u32, dt: f32, settings: WetSettings) -> Self {
        Self {
            width,
            height,
            dt,
            diffusion: settings.diffusion,
            evaporation: settings.evaporation,
            absorption: settings.absorption,
            edge: settings.edge,
            advection: settings.advection,
            pigment_diffusion: settings.pigment_diffusion,
            dry: settings.dry,
            count: 0,
            colour_r: 0.0,
            colour_g: 0.0,
            colour_b: 0.0,
            colour_a: 0.0,
        }
    }
}
