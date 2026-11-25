//! What the paint app does that does not depend on a window.
//!
//! The split is deliberate: everything here is testable without a GPU, and every platform shell -
//! SwiftUI on macOS and iPad, Compose on Android, GTK on Linux, WinUI on Windows - is a thin
//! layer over these types. A feature that cannot be expressed here is a feature that would have to
//! be written four times.

pub mod brush;
pub mod color;
pub mod color_space;
pub mod document;
pub mod ffi;
pub mod ffi_hdr;
pub mod fill;
pub mod fill_mask;
pub mod history;
pub mod l10n;
pub mod layer_png;
pub mod picker;
pub mod predict;
pub mod record;
pub mod shape;
pub mod surface;
pub mod tools;
