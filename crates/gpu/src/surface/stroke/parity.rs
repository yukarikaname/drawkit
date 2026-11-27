//! CPU/GPU brush parity for the complex stamp paths.
//!
//! Paints the same segments twice - once through `drawkit_core::brush` into a `Surface`, once
//! through the offscreen `GpuCompositor` (the coverage compute shader followed by the tile commit
//! shader) - and compares the pixels the commit leaves in the resident layer. This is the smallest
//! entry point that runs the real commit shader without a window: `GpuCompositor::new` is
//! offscreen, and `materialize` reads the straight RGBA8 tiles back. The extended-range case reads
//! `composite_linear` instead, which is the same commit into an Rgba16Float tile.
//!
//! The comparison is straight colour weighted by alpha, so a hidden colour under a nearly
//! transparent pixel is not counted; visible differences are held to 1/255.

mod compare;
mod complex;
mod paint;
mod simple;
mod tile_edge;
