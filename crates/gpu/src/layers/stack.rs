use std::collections::{HashMap, HashSet};

use crate::composite::{Blend, ClipSource, FORMAT, Source, TileResources, TileTargets};

/// The largest tile edge. Smaller canvases use one tile of the canvas size.
const MAX_TILE: u32 = 1024;

pub(super) mod commit;
mod compose;

struct TileTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bytes: usize,
}

struct Tile {
    backup: Vec<u8>,
    nonzero: usize,
    gpu: Option<TileTexture>,
    used: u64,
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
    tile_width: u32,
    tile_height: u32,
    layers: Vec<Option<Layer>>,
    budget_bytes: usize,
    resident_bytes: usize,
    clock: u64,
    /// The last composed canvas, kept between presents so a stroke only recomposes its tiles.
    output: Option<wgpu::Texture>,
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
        let tile_width = width.clamp(1, MAX_TILE);
        let tile_height = height.clamp(1, MAX_TILE);
        Self {
            width,
            height,
            tile_width,
            tile_height,
            layers: Vec::new(),
            budget_bytes: 1 << 30,
            resident_bytes: 0,
            clock: 0,
            output: None,
            dirty_tiles: HashSet::new(),
        }
    }

    pub fn set_budget(&mut self, bytes: usize) {
        let tile = self.tile_width as usize * self.tile_height as usize * 4;
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
        self.tile_width as usize * self.tile_height as usize * 4
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

    /// Upload one layer's straight RGBA8 pixels. Only tiles with a non-zero byte get a backup;
    /// GPU textures are created lazily by the compositor.
    pub fn set(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        index: usize,
        pixels: &[u8],
    ) -> bool {
        let expected = self.width as usize * self.height as usize * 4;
        if self.width == 0 || self.height == 0 || pixels.len() < expected {
            return false;
        }
        if self.layers.len() <= index {
            self.layers.resize_with(index + 1, || None);
        }
        self.drop_layer(index);
        self.mark_all_dirty();
        let tile_width = self.tile_width as usize;
        let tile_height = self.tile_height as usize;
        let tile_bytes = tile_width * tile_height * 4;
        let mut layer = Layer {
            tiles: HashMap::new(),
            known: true,
        };
        for ty in 0..self.height.div_ceil(self.tile_height) {
            for tx in 0..self.width.div_ceil(self.tile_width) {
                let origin_x = tx * self.tile_width;
                let origin_y = ty * self.tile_height;
                let width = (self.width - origin_x).min(self.tile_width) as usize;
                let height = (self.height - origin_y).min(self.tile_height) as usize;
                let mut backup = vec![0u8; tile_bytes];
                let mut nonzero = 0;
                for row in 0..height {
                    let source =
                        ((origin_y as usize + row) * self.width as usize + origin_x as usize) * 4;
                    let target = row * tile_width * 4;
                    backup[target..target + width * 4]
                        .copy_from_slice(&pixels[source..source + width * 4]);
                    nonzero += (0..width)
                        .filter(|column| nonzero_pixel(&pixels[source + column * 4..][..4]))
                        .count();
                }
                if nonzero > 0 {
                    layer.tiles.insert(
                        (tx, ty),
                        Tile {
                            backup,
                            nonzero,
                            gpu: None,
                            used: 0,
                        },
                    );
                }
            }
        }
        self.layers[index] = Some(layer);
        true
    }

    /// Upload one dirty rectangle of a layer. Resident GPU tiles are written in place; a cold
    /// tile keeps the new pixels in its backup and uploads them before the next composite.
    pub fn update_rect(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        request: RectUpload<'_>,
    ) -> bool {
        let RectUpload {
            index,
            x,
            y,
            width,
            height,
            pixels,
        } = request;
        if width == 0
            || height == 0
            || x + width > self.width
            || y + height > self.height
            || pixels.len() < (width * height * 4) as usize
        {
            return false;
        }
        if self.layers.len() <= index {
            self.layers.resize_with(index + 1, || None);
        }
        if self.layers[index].is_none() {
            self.layers[index] = Some(Layer {
                tiles: HashMap::new(),
                known: true,
            });
        }
        let tile_width = self.tile_width;
        let tile_height = self.tile_height;
        let tile_columns = tile_width as usize;
        let tile_bytes = self.tile_bytes();
        let tick = self.next_tick();
        let stride = width as usize * 4;
        let right = x + width;
        let bottom = y + height;
        let layer = self.layers[index].as_mut().expect("layer just inserted");
        layer.known = true;
        let mut freed = 0;
        for ty in y / tile_height..=(bottom - 1) / tile_height {
            for tx in x / tile_width..=(right - 1) / tile_width {
                let left = x.max(tx * tile_width);
                let top = y.max(ty * tile_height);
                let limit_x = right.min((tx + 1) * tile_width);
                let limit_y = bottom.min((ty + 1) * tile_height);
                if limit_x <= left || limit_y <= top {
                    continue;
                }
                let clip_width = (limit_x - left) as usize;
                let clip_height = (limit_y - top) as usize;
                self.dirty_tiles.insert((tx, ty));
                let key = (tx, ty);
                let remove;
                {
                    let tile = layer.tiles.entry(key).or_insert_with(|| Tile {
                        backup: vec![0; tile_bytes],
                        nonzero: 0,
                        gpu: None,
                        used: 0,
                    });
                    for row in 0..clip_height {
                        let source =
                            ((row + (top - y) as usize) * width as usize + (left - x) as usize) * 4;
                        let local_y = (top - ty * tile_height) as usize + row;
                        let target =
                            (local_y * tile_columns + (left - tx * tile_width) as usize) * 4;
                        for column in 0..clip_width {
                            let old = &tile.backup[target + column * 4..][..4];
                            let new = &pixels[source + column * 4..][..4];
                            match (nonzero_pixel(old), nonzero_pixel(new)) {
                                (false, true) => tile.nonzero += 1,
                                (true, false) => tile.nonzero -= 1,
                                _ => {}
                            }
                        }
                        tile.backup[target..target + clip_width * 4]
                            .copy_from_slice(&pixels[source..source + clip_width * 4]);
                    }
                    if let Some(gpu) = &tile.gpu {
                        tile.used = tick;
                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &gpu.texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d {
                                    x: left - tx * tile_width,
                                    y: top - ty * tile_height,
                                    z: 0,
                                },
                                aspect: wgpu::TextureAspect::All,
                            },
                            &pixels
                                [((top - y) as usize * width as usize + (left - x) as usize) * 4..],
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(stride as u32),
                                rows_per_image: Some(clip_height as u32),
                            },
                            wgpu::Extent3d {
                                width: clip_width as u32,
                                height: clip_height as u32,
                                depth_or_array_layers: 1,
                            },
                        );
                    }
                    remove = tile.nonzero == 0;
                }
                if remove
                    && let Some(tile) = layer.tiles.remove(&key)
                    && let Some(gpu) = tile.gpu
                {
                    freed += gpu.bytes;
                }
            }
        }
        self.resident_bytes = self.resident_bytes.saturating_sub(freed);
        true
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
            format: FORMAT,
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
                bytes_per_row: Some(width * 4),
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
                    if tile.gpu.is_none() || Some(*coord) == keep {
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
