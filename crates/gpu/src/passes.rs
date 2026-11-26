//! Standalone GPU passes, each next to the shader it runs.
//!
//! They are re-exported at the crate root, so call sites keep using `crate::morphology` and the
//! like; the point of the folder is that the flat `src/` holds the renderer, the layers and the
//! surface, not every pass.

pub(crate) mod brush;
pub(crate) mod color;
pub(crate) mod composite;
pub(crate) mod decontaminate;
pub(crate) mod feather;
pub(crate) mod morphology;
pub(crate) mod outline;
pub(crate) mod selection;
pub(crate) mod thumbnail;
