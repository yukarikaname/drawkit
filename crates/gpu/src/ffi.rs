//! The C surface for the GPU drawing figure.
//!
//! A handle owns the figure and the wgpu renderer together: a pose change and a render have to use
//! the same device, and splitting them would let a shell keep a device alive longer than the
//! validation layer expects. There is no CPU path behind this; if the machine has no adapter the
//! handle is null and the shell draws no figure.

mod canvas;
mod compositor;
mod figure;
mod stroke;
