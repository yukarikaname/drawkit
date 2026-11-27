// Keeping the resident layer stack in step with the document.
//
// The swapchain-like surface holds the undo journal in its tiles, so the stack is rebuilt only
// when the layer count or the canvas size changed; everything else is a spec update, which the
// surface skips when nothing moved.

use drawkit_core::document::{BlendMode, LayerKind};
use drawkit_core::surface::Rect;

use crate::compositor::State;
use crate::pixels::{layer_linear, layer_rgba};

impl State {
    /// Upload the document to the surface. False means the layer path cannot run, so the canvas
    /// shows nothing and the status bar says why.
    pub(crate) fn sync_gpu(&mut self) -> bool {
        if !self.layers_supported() {
            return false;
        }
        let count = self.editor.document.layers.len();
        let (width, height) = (self.width, self.height);
        let rebuild = self.surface_layers != count || self.surface_size != (width, height);
        let Some(paint) = self.paint.as_mut() else {
            return false;
        };
        if !paint.supports_layers() {
            return false;
        }
        paint.set_layer_budget(1536 << 20);
        if rebuild {
            paint.reset_layers(count, width, height);
            // An extended-range stack composes half floats, so its layers are uploaded as linear
            // Display P3; an eight-bit stack takes the document-space bytes as they are.
            let hdr = self.hdr;
            let space = self.editor.document.color_space;
            for (index, layer) in self.editor.document.layers.iter().enumerate() {
                if layer.kind != LayerKind::Paint {
                    continue;
                }
                let rect = Rect::new(0, 0, width as usize, height as usize);
                let uploaded = if hdr {
                    let pixels = layer_linear(layer, rect, space);
                    paint.upload_layer_linear(index, 0, 0, width, height, &pixels)
                } else {
                    let pixels = layer_rgba(layer, rect);
                    paint.upload_layer(index, 0, 0, width, height, &pixels)
                };
                if !uploaded {
                    return false;
                }
            }
            self.surface_layers = count;
            self.surface_size = (width, height);
        }
        let document = &self.editor.document;
        for (index, layer) in document.layers.iter().enumerate() {
            paint.set_layer(
                index,
                document.effective_opacity(index),
                layer.blend.as_i32(),
                document.effective_visible(index),
                (layer.kind == LayerKind::Fill).then_some(layer.fill),
            );
            paint.set_layer_parent(index, layer.parent);
            paint.set_layer_clip(index, document.clip_base(index));
        }
        true
    }

    /// Groups with their own opacity or blend, a clipping layer on a group, and a child of a
    /// group with a blend mode are not reproduced by the layer stack. The shell says so and
    /// shows nothing rather than a frame that is not the document.
    fn layers_supported(&self) -> bool {
        let document = &self.editor.document;
        for (index, layer) in document.layers.iter().enumerate() {
            if layer.kind == LayerKind::Group
                && (layer.opacity < 0.999 || layer.blend != BlendMode::Normal || layer.clipping)
            {
                return false;
            }
            if layer.parent.is_some() && layer.blend != BlendMode::Normal {
                return false;
            }
            if layer.clipping && document.clip_base(index).is_none() {
                return false;
            }
        }
        true
    }
}
