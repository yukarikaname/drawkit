//! The GPU path: painting layers, the figure and the window surface.
//!
//! `Renderer` owns one wgpu device and the pipelines. `LayerStack` keeps the painting layers as
//! tiles, `GpuCompositor` exposes the same stack to shells that present through a bitmap, and
//! `CanvasSurface` adds the swapchain and the paper shader. The same crate builds on Metal, D3D12,
//! Vulkan and GLES; the window handle is the only platform-specific input.

mod brush;
mod color;
mod composite;
mod compositor;
mod decontaminate;
mod feather;
mod ffi;
mod layers;
mod morphology;
mod outline;
mod readback;
mod renderer;
mod selection;
mod surface;
mod thumbnail;

pub use brush::Segment as BrushSegment;
pub use color::Operation as ColorOperation;
pub use composite::{Blend, Layer};
pub use compositor::GpuCompositor;
pub use layers::{LayerSpec, LayerStack, RectUpload};
pub use morphology::Morphology;
pub use readback::PendingReadback;
pub use renderer::Renderer;
