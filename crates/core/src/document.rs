//! Layers, and what the screen shows.
//!
//! A document is a stack of surfaces and one rule for combining them. The shell never asks for the
//! whole picture: it asks for the rectangle it is about to upload, which is the same rectangle the
//! stroke dirtied - so compositing costs what the brush touched, not what the canvas.
//!
//! Groups are nodes in the same flat stack. A layer's `parent` points at its group's index; the
//! composite walks the parent relation so a group's opacity applies to the group's own result, not
//! once per child. The flat indices are also what the undo history and the C API use.

use crate::color::{Rgba, over};
use crate::surface::{Rect, Surface};

mod composite;
mod layers;

pub use crate::color_space::{ColorSpace, DynamicRange};

/// How a layer's colour combines with what is already below it. Alpha stays ordinary: only the
/// colour changes, which is what makes a multiply layer a shadow rather than a darker rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Add,
    Darken,
    Lighten,
    Color,
    Luminosity,
}

impl BlendMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Multiply,
            2 => Self::Screen,
            3 => Self::Overlay,
            4 => Self::Add,
            5 => Self::Darken,
            6 => Self::Lighten,
            7 => Self::Color,
            8 => Self::Luminosity,
            _ => Self::Normal,
        }
    }

    pub fn as_i32(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::Multiply => 1,
            Self::Screen => 2,
            Self::Overlay => 3,
            Self::Add => 4,
            Self::Darken => 5,
            Self::Lighten => 6,
            Self::Color => 7,
            Self::Luminosity => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Paint,
    Group,
    Fill,
}

#[derive(Clone)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub kind: LayerKind,
    pub blend: BlendMode,
    pub locked: bool,
    /// Paint only where the layer already has alpha, and never change that alpha.
    pub alpha_locked: bool,
    /// Show this layer only through the alpha of the nearest layer below it that is not clipped.
    pub clipping: bool,
    pub parent: Option<usize>,
    pub expanded: bool,
    /// Used by `LayerKind::Fill`; paint and group layers ignore it.
    pub fill: Rgba,
    pub surface: Surface,
}

impl Layer {
    pub fn new(name: impl Into<String>, width: usize, height: usize) -> Self {
        Self {
            name: name.into(),
            visible: true,
            opacity: 1.0,
            kind: LayerKind::Paint,
            blend: BlendMode::Normal,
            locked: false,
            alpha_locked: false,
            clipping: false,
            parent: None,
            expanded: true,
            fill: [0.0; 4],
            surface: Surface::new(width, height),
        }
    }

    pub fn group(name: impl Into<String>, width: usize, height: usize) -> Self {
        Self {
            name: name.into(),
            visible: true,
            opacity: 1.0,
            kind: LayerKind::Group,
            blend: BlendMode::Normal,
            locked: false,
            alpha_locked: false,
            clipping: false,
            parent: None,
            expanded: true,
            fill: [0.0; 4],
            surface: Surface::new(width, height),
        }
    }

    pub fn fill(name: impl Into<String>, color: Rgba, width: usize, height: usize) -> Self {
        Self {
            name: name.into(),
            visible: true,
            opacity: 1.0,
            kind: LayerKind::Fill,
            blend: BlendMode::Normal,
            locked: false,
            alpha_locked: false,
            clipping: false,
            parent: None,
            expanded: true,
            fill: color,
            surface: Surface::new(width, height),
        }
    }
}

/// The part of a layer that is not pixels: the palette state and the tree links. Layer operations
/// undo by restoring these without copying a surface.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerMeta {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub kind: LayerKind,
    pub blend: BlendMode,
    pub locked: bool,
    pub alpha_locked: bool,
    pub clipping: bool,
    pub parent: Option<usize>,
    pub expanded: bool,
    pub fill: Rgba,
}

impl Layer {
    pub fn meta(&self) -> LayerMeta {
        LayerMeta {
            name: self.name.clone(),
            visible: self.visible,
            opacity: self.opacity,
            kind: self.kind,
            blend: self.blend,
            locked: self.locked,
            alpha_locked: self.alpha_locked,
            clipping: self.clipping,
            parent: self.parent,
            expanded: self.expanded,
            fill: self.fill,
        }
    }

    pub fn apply_meta(&mut self, meta: &LayerMeta) {
        self.name = meta.name.clone();
        self.visible = meta.visible;
        self.opacity = meta.opacity;
        self.kind = meta.kind;
        self.blend = meta.blend;
        self.locked = meta.locked;
        self.alpha_locked = meta.alpha_locked;
        self.clipping = meta.clipping;
        self.parent = meta.parent;
        self.expanded = meta.expanded;
        self.fill = meta.fill;
    }
}

/// The two halves of a layer deletion: what left the stack, and the fallback created when nothing
/// paintable was left. Undo needs both to put the tree back and take the fallback away again.
#[derive(Clone)]
pub struct LayerRemoval {
    pub removed: Vec<(usize, Layer)>,
    pub inserted: Vec<(usize, Layer)>,
    pub active_before: usize,
    pub active_after: usize,
}

#[derive(Clone)]
pub struct Document {
    width: usize,
    height: usize,
    pub color_space: ColorSpace,
    pub dynamic_range: DynamicRange,
    pub layers: Vec<Layer>,
    pub active: usize,
}

/// The luminance weight Photoshop and the PDF blend model use, so a luminosity layer looks the
/// same here as in another app.
impl Document {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            color_space: ColorSpace::Srgb,
            dynamic_range: DynamicRange::Sdr,
            layers: vec![Layer::new("Layer 1", width, height)],
            active: 0,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn active(&self) -> &Layer {
        &self.layers[self.active]
    }

    pub fn active_mut(&mut self) -> &mut Layer {
        &mut self.layers[self.active]
    }

    pub fn is_group(&self, index: usize) -> bool {
        self.layers
            .get(index)
            .is_some_and(|layer| layer.kind == LayerKind::Group)
    }

    /// True when the active item can receive paint.
    pub fn can_paint(&self) -> bool {
        self.layers
            .get(self.active)
            .is_some_and(|layer| layer.kind == LayerKind::Paint && !layer.locked)
    }

    pub fn depth(&self, index: usize) -> usize {
        let mut depth = 0;
        let mut parent = self.layers.get(index).and_then(|layer| layer.parent);
        while let Some(group) = parent {
            depth += 1;
            parent = self.layers.get(group).and_then(|layer| layer.parent);
        }
        depth
    }

    /// Visible through every ancestor.
    pub fn effective_visible(&self, index: usize) -> bool {
        let mut current = Some(index);
        while let Some(layer) = current.and_then(|i| self.layers.get(i)) {
            if !layer.visible {
                return false;
            }
            current = layer.parent;
        }
        true
    }

    /// The product of every ancestor's opacity, including this layer's own.
    pub fn effective_opacity(&self, index: usize) -> f32 {
        let mut opacity = 1.0;
        let mut current = Some(index);
        while let Some(layer) = current.and_then(|i| self.layers.get(i)) {
            opacity *= layer.opacity.clamp(0.0, 1.0);
            current = layer.parent;
        }
        opacity
    }

    /// Add a paint layer above the active item. If a group is active the layer goes inside it;
    /// otherwise it becomes a sibling.
    /// True when every paint layer is empty. Groups and empty containers do not count as content.
    pub fn is_clear(&self) -> bool {
        self.layers.iter().all(|layer| match layer.kind {
            LayerKind::Paint => layer.surface.is_clear(),
            LayerKind::Group => true,
            LayerKind::Fill => layer.fill[3] <= 0.0,
        })
    }

    /// Every paint layer's dirty rectangle, as one rectangle over the whole document.
    pub fn dirty(&self) -> Option<Rect> {
        self.layers
            .iter()
            .filter(|layer| layer.kind == LayerKind::Paint)
            .filter_map(|layer| layer.surface.dirty())
            .reduce(|a, b| a.union(&b))
    }

    pub fn take_dirty(&mut self) -> Option<Rect> {
        self.layers
            .iter_mut()
            .filter(|layer| layer.kind == LayerKind::Paint)
            .filter_map(|layer| layer.surface.take_dirty())
            .reduce(|a, b| a.union(&b))
    }
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
