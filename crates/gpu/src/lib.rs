//! The GPU renderer for the drawing mannequin.
//!
//! This is the first slice of the GPU path: a headless render target and one pipeline, so the
//! figure can be drawn without a window and without a platform-specific surface. The same device,
//! queue and pipeline shape will later take the painting layers; the difference is a surface and
//! more passes, not another renderer.

mod brush;
mod color;
mod composite;
mod decontaminate;
mod feather;
mod ffi;
mod layers;
mod morphology;
mod readback;
mod renderer;
mod selection;
mod surface;
mod thumbnail;

pub use brush::Segment as BrushSegment;
pub use color::Operation as ColorOperation;
pub use composite::{Blend, Layer};
pub use layers::{LayerSpec, LayerStack, RectUpload};
pub use morphology::Morphology;
pub use readback::PendingReadback;
pub use renderer::Renderer;
