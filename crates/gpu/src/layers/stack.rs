use std::collections::{HashMap, HashSet};

use crate::composite::{Blend, ClipSource, FORMAT, Source, TileResources, TileTargets};

/// The largest tile edge. Smaller canvases use one tile of the canvas size.
const MAX_TILE: u32 = 1024;

mod access;
pub(super) mod commit;
mod compose;
mod upload;

pub(crate) struct TileTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bytes: usize,
}

struct Tile {
    backup: Vec<u8>,
    nonzero: usize,
    gpu: Option<TileTexture>,
    used: u64,
    /// The GPU texture changed without the CPU backup following it yet.
    stale: bool,
}

struct Layer {
    tiles: HashMap<(u32, u32), Tile>,
    /// A layer uploaded as fully transparent still has no tiles, but it must not be re-uploaded
    /// by the shell on every frame.
    known: bool,
}

pub struct LayerStack {
    width: u32,
    height: u32,
    /// Format of every resident tile; eight-bit today, a half float for an HDR document.
    format: wgpu::TextureFormat,
    tile_width: u32,
    tile_height: u32,
    layers: Vec<Option<Layer>>,
    budget_bytes: usize,
    resident_bytes: usize,
    clock: u64,
    /// The last composed canvas, kept between presents so a stroke only recomposes its tiles.
    output: Option<wgpu::Texture>,
    /// The same canvas with the active layer left out, for an erase preview.
    output_skip: Option<wgpu::Texture>,
    dirty_tiles: HashSet<(u32, u32)>,
}

pub struct RectUpload<'a> {
    pub index: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

/// The same rectangle in straight linear Display P3 floats, for an extended-range stack.
pub struct LinearUpload<'a> {
    pub index: usize,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [f32],
}

/// Which resident layer to composite, and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerSpec {
    pub layer: usize,
    pub opacity: f32,
    pub visible: bool,
    pub blend: Blend,
    /// A fill layer supplies its colour here and has no texture.
    pub fill: Option<[f32; 4]>,
    /// Show this layer only through the alpha of the layer below it.
    pub clip: Option<usize>,
}

impl LayerSpec {
    pub fn new(layer: usize) -> Self {
        Self {
            layer,
            opacity: 1.0,
            visible: true,
            blend: Blend::Normal,
            fill: None,
            clip: None,
        }
    }

    pub fn blend(mut self, blend: Blend) -> Self {
        self.blend = blend;
        self
    }

    pub fn fill(mut self, fill: [f32; 4]) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn clip(mut self, layer: usize) -> Self {
        self.clip = Some(layer);
        self
    }
}

fn nonzero_pixel(pixel: &[u8]) -> bool {
    pixel[0] | pixel[1] | pixel[2] | pixel[3] != 0
}

impl LayerStack {
    pub fn new(width: u32, height: u32) -> Self {
        Self::with_format(width, height, crate::composite::FORMAT)
    }

    /// A stack whose tiles are `format`. Eight-bit documents keep `Rgba8Unorm`; an extended-range
    /// surface uses `Rgba16Float` and uploads premultiplied linear values instead.
    pub fn with_format(width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let tile_width = width.clamp(1, MAX_TILE);
        let tile_height = height.clamp(1, MAX_TILE);
        Self {
            width,
            height,
            format,
            tile_width,
            tile_height,
            layers: Vec::new(),
            budget_bytes: 1 << 30,
            resident_bytes: 0,
            clock: 0,
            output: None,
            output_skip: None,
            dirty_tiles: HashSet::new(),
        }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    pub fn bytes_per_pixel(&self) -> usize {
        if self.format == crate::composite::FORMAT {
            4
        } else {
            8
        }
    }

    pub fn set_budget(&mut self, bytes: usize) {
        let tile = self.tile_width as usize * self.tile_height as usize * self.bytes_per_pixel();
        self.budget_bytes = bytes.max(tile);
        self.evict_cold(None);
    }

    pub fn budget(&self) -> usize {
        self.budget_bytes
    }

    pub fn resident_bytes(&self) -> usize {
        self.resident_bytes
    }

    /// Whether the shell still needs to upload this layer. Evicted tiles are rehydrated from
    /// their backups, so a known layer is never missing.
    pub fn layer_missing(&self, index: usize) -> bool {
        self.layers
            .get(index)
            .is_none_or(|layer| layer.as_ref().is_none_or(|l| !l.known))
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.iter().all(Option::is_none)
    }

    fn coords(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        let columns = self.width.div_ceil(self.tile_width);
        let rows = self.height.div_ceil(self.tile_height);
        (0..rows).flat_map(move |y| (0..columns).map(move |x| (x, y)))
    }

    fn tile_bytes(&self) -> usize {
        self.tile_width as usize * self.tile_height as usize * self.bytes_per_pixel()
    }

    fn next_tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Everything that changes the whole stack (a new layer, a reorder, a layer spec) marks every
    /// tile. A stroke only marks the tiles its dirty rectangle touched.
    pub fn mark_all_dirty(&mut self) {
        let coords: Vec<_> = self.coords().collect();
        self.dirty_tiles.extend(coords);
    }

    /// Clear one layer's tiles and its GPU residency.
    fn drop_layer(&mut self, index: usize) {
        let freed = match self.layers.get_mut(index) {
            Some(slot) => slot.take().map_or(0, |layer| {
                layer
                    .tiles
                    .into_values()
                    .filter_map(|tile| tile.gpu.map(|gpu| gpu.bytes))
                    .sum()
            }),
            None => 0,
        };
        self.resident_bytes = self.resident_bytes.saturating_sub(freed);
    }

    pub fn remove(&mut self, index: usize) {
        self.drop_layer(index);
        self.mark_all_dirty();
    }

    /// Materialise a tile on the GPU from its backup. Returns false when the tile is empty.
    fn ensure_tile(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        index: usize,
        coord: (u32, u32),
    ) -> bool {
        let tick = self.next_tick();
        let (width, height) = (self.tile_width, self.tile_height);
        let bytes = self.tile_bytes();
        let stride = width * self.bytes_per_pixel() as u32;
        let Some(layer) = self.layers.get_mut(index).and_then(Option::as_mut) else {
            return false;
        };
        let Some(tile) = layer.tiles.get_mut(&coord) else {
            return false;
        };
        tile.used = tick;
        if tile.gpu.is_some() {
            return true;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tile texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &tile.backup,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&Default::default());
        tile.gpu = Some(TileTexture {
            texture,
            view,
            bytes,
        });
        self.resident_bytes += bytes;
        self.evict_cold(Some(coord));
        true
    }

    /// Drop the coldest GPU tiles until the stack fits, never dropping the tile being rendered.
    fn evict_cold(&mut self, keep: Option<(u32, u32)>) {
        while self.resident_bytes > self.budget_bytes {
            let mut victim: Option<(usize, (u32, u32), u64)> = None;
            for (index, layer) in self.layers.iter().enumerate() {
                let Some(layer) = layer else { continue };
                for (coord, tile) in &layer.tiles {
                    if tile.gpu.is_none() || tile.stale || Some(*coord) == keep {
                        continue;
                    }
                    if victim.is_none_or(|(_, _, used)| tile.used < used) {
                        victim = Some((index, *coord, tile.used));
                    }
                }
            }
            let Some((index, coord, _)) = victim else {
                break;
            };
            let freed = self
                .layers
                .get_mut(index)
                .and_then(Option::as_mut)
                .and_then(|layer| layer.tiles.get_mut(&coord))
                .and_then(|tile| tile.gpu.take())
                .map_or(0, |gpu| gpu.bytes);
            self.resident_bytes = self.resident_bytes.saturating_sub(freed);
        }
    }
}
