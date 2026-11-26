//! The smart fill edge cache: one model run per document revision.

use super::*;

/// One canvas-sized edge map for the document revision it was built from. A smart fill reads it
/// again when only the seed, tolerance or threshold changed; any pixel or layer change replaces it.
#[derive(Clone, Default)]
pub(crate) struct SmartEdgeCache {
    revision: Option<u64>,
    width: usize,
    height: usize,
    edges: Option<Vec<f32>>,
    /// How many times the edge model has run for this editor, for diagnostics and tests.
    builds: u64,
}

impl Editor {
    /// How many times the edge model has run for the fill cache on this editor. A repeated smart
    /// fill at an unchanged document reads the cached map instead of running TEED again.
    pub fn fill_edge_builds(&self) -> u64 {
        self.fill_edges.builds
    }

    /// The number of edge probabilities the fill cache holds, or zero when it holds none. There is
    /// at most one document-sized map, so this is one canvas area when the model is available.
    pub fn fill_edge_len(&self) -> usize {
        self.fill_edges.edges.as_ref().map_or(0, Vec::len)
    }

    /// The document revision the cache is keyed by: every layer's write generation plus the layer
    /// metadata the composite reads. Any paint write or layer change moves it, so the edge map is
    /// only reused for a byte-identical picture. This is the whole-document revision the spatial
    /// state notes call for; per-region invalidation can come later if it is ever a bottleneck.
    fn document_revision(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut mix = |value: u64| {
            hash ^= value;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        mix(self.document.width() as u64);
        mix(self.document.height() as u64);
        mix(self.document.layers.len() as u64);
        for layer in &self.document.layers {
            mix(layer.surface.generation());
            mix(match layer.kind {
                LayerKind::Paint => 1,
                LayerKind::Group => 2,
                LayerKind::Fill => 3,
            });
            mix(if layer.visible { 1 } else { 0 });
            mix(u64::from(layer.opacity.to_bits()));
            mix(layer.blend.as_i32() as u64);
            mix(if layer.clipping { 1 } else { 0 });
            mix(if layer.reference { 1 } else { 0 });
            mix(layer.parent.map_or(0, |parent| parent as u64 + 1));
            for channel in layer.fill {
                mix(u64::from(channel.to_bits()));
            }
        }
        hash
    }

    /// The edge map for `composite`, built once per document revision. The model is behind
    /// `drawkit-lineart`; when it is unavailable the plan falls back to morphology alone.
    pub(super) fn smart_edges(
        &mut self,
        composite: &[Rgba],
        width: usize,
        height: usize,
    ) -> Option<&[f32]> {
        let revision = self.document_revision();
        let current = self.fill_edges.revision == Some(revision)
            && self.fill_edges.width == width
            && self.fill_edges.height == height;
        if !current {
            let edges = crate::fill::flat_edges(composite, width, height);
            self.fill_edges = SmartEdgeCache {
                revision: Some(revision),
                width,
                height,
                edges,
                builds: self.fill_edges.builds + 1,
            };
        }
        self.fill_edges.edges.as_deref()
    }
}
