//! Layers that stay on the GPU, stored as tiles.
//!
//! A full-canvas texture per layer is fine for a sketch and wrong for an 8K painting: 7680×4320
//! RGBA8 is 132 MB for one layer. A `LayerStack` now splits each layer into square tiles, keeps a
//! CPU backup for the tiles that have pixels, and only materialises a GPU texture when the
//! compositor needs that tile. The byte budget evicts the least-recently-used GPU tiles; the
//! backups rehydrate them, so the shell only uploads a layer once.

mod journal;
mod residency;
pub(crate) mod specs;
mod stack;
pub(crate) use journal::TileJournal;
pub(crate) use residency::TileResidency;
pub(crate) use stack::commit::TileSwap;
pub use stack::commit::{
    CommittedTile, LinearTile, pipeline as commit_pipeline, pipeline_for as commit_pipeline_for,
};
pub use stack::{LayerSpec, LayerStack, LinearUpload, RectUpload};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_layers;
#[cfg(test)]
mod tests_rect;
