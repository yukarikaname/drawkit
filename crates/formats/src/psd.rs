use std::path::Path;

use crate::{Error, ExportDocument, ImportedDocument, ImportedLayer, error};

/// Read a `.psd` from disk.
pub fn read_psd(path: &Path) -> Result<ImportedDocument, Error> {
    let bytes =
        std::fs::read(path).map_err(|e| error(format!("reading {}: {e}", path.display())))?;
    read_psd_bytes(&bytes)
}

/// Read a `.psd` from memory. Groups are flattened; blend modes other than normal are ignored
/// rather than guessed, because the editor has one blend mode today.
pub fn read_psd_bytes(bytes: &[u8]) -> Result<ImportedDocument, Error> {
    let options = ag_psd::psd::ReadOptions {
        use_image_data: Some(true),
        ..Default::default()
    };
    let psd = ag_psd::reader::read_psd(bytes, &options)
        .map_err(|e| error(format!("parsing the PSD: {e:?}")))?;
    let width = psd.width.round() as usize;
    let height = psd.height.round() as usize;
    if width == 0 || height == 0 {
        return Err(error("the PSD has no canvas"));
    }

    let mut layers = Vec::new();
    if let Some(children) = &psd.children {
        collect_psd_layers(children, width, height, true, 1.0, &mut layers);
    }
    if layers.is_empty()
        && let Some(data) = psd.image_data.as_ref().or(psd.canvas.as_ref())
    {
        let mut canvas = vec![0_u8; width * height * 4];
        blit(
            &data.data,
            (data.width as usize, data.height as usize),
            (0, 0),
            &mut canvas,
            (width, height),
        );
        layers.push(ImportedLayer {
            name: "Merged".into(),
            visible: true,
            opacity: 1.0,
            x: 0,
            y: 0,
            width,
            height,
            rgba: canvas,
        });
    }
    if layers.is_empty() {
        return Err(error("the PSD has no raster layers"));
    }
    layers.reverse();
    Ok(ImportedDocument {
        width,
        height,
        color_space: 0,
        layers,
    })
}

fn collect_psd_layers(
    children: &[ag_psd::psd::Layer],
    width: usize,
    height: usize,
    parent_visible: bool,
    parent_opacity: f32,
    out: &mut Vec<ImportedLayer>,
) {
    for layer in children {
        let visible = parent_visible && !layer.hidden.unwrap_or(false);
        let opacity = parent_opacity * layer.opacity.unwrap_or(1.0) as f32;
        if let Some(children) = &layer.children {
            collect_psd_layers(children, width, height, visible, opacity, out);
            continue;
        }
        let Some(data) = layer.image_data.as_ref().or(layer.canvas.as_ref()) else {
            continue;
        };
        let layer_width = data.width as usize;
        let layer_height = data.height as usize;
        if layer_width == 0 || layer_height == 0 || data.data.len() < layer_width * layer_height * 4
        {
            continue;
        }
        let left = layer.left.unwrap_or(0.0).round() as i32;
        let top = layer.top.unwrap_or(0.0).round() as i32;
        let mut canvas = vec![0_u8; width * height * 4];
        blit(
            &data.data,
            (layer_width, layer_height),
            (left, top),
            &mut canvas,
            (width, height),
        );
        out.push(ImportedLayer {
            name: layer
                .additional_info
                .name
                .clone()
                .unwrap_or_else(|| "Layer".into()),
            visible,
            opacity,
            x: 0,
            y: 0,
            width,
            height,
            rgba: canvas,
        });
    }
}

/// Write a PSD with one raster layer per painting layer. Groups and smart objects are not
/// invented; the flattened composite is included so other editors show the right image.
pub fn write_psd(document: &ExportDocument) -> Result<Vec<u8>, Error> {
    use ag_psd::psd::{ColorMode, Layer, LayerAdditionalInfo, PixelData, Psd, WriteOptions};

    if document.width == 0 || document.height == 0 || document.layers.is_empty() {
        return Err(error("cannot write an empty PSD document"));
    }
    let width = document.width as u32;
    let height = document.height as u32;
    let composite = flatten(document);
    let mut layers = Vec::new();
    for layer in document.layers.iter().rev() {
        if layer.rgba.len() < document.width * document.height * 4 {
            return Err(error(format!("layer {} is not document-sized", layer.name)));
        }
        layers.push(Layer {
            additional_info: LayerAdditionalInfo {
                name: Some(layer.name.clone()),
                ..Default::default()
            },
            top: Some(0.0),
            left: Some(0.0),
            bottom: Some(document.height as f64),
            right: Some(document.width as f64),
            hidden: Some(!layer.visible),
            opacity: Some(layer.opacity.clamp(0.0, 1.0) as f64),
            image_data: Some(PixelData {
                width,
                height,
                data: layer.rgba.clone(),
            }),
            ..Default::default()
        });
    }
    let psd = Psd {
        width: document.width as f64,
        height: document.height as f64,
        channels: Some(4.0),
        bits_per_channel: Some(8.0),
        color_mode: Some(ColorMode::Rgb),
        children: Some(layers),
        image_data: Some(PixelData {
            width,
            height,
            data: composite,
        }),
        ..Default::default()
    };
    Ok(ag_psd::writer::write_psd(
        &psd,
        &WriteOptions {
            compress: Some(true),
            ..Default::default()
        },
    ))
}

fn flatten(document: &ExportDocument) -> Vec<u8> {
    let mut out = vec![0.0_f32; document.width * document.height * 4];
    for layer in &document.layers {
        if !layer.visible || layer.opacity <= 0.0 {
            continue;
        }
        for (index, pixel) in layer.rgba.as_chunks::<4>().0.iter().enumerate() {
            let alpha = pixel[3] as f32 / 255.0 * layer.opacity;
            if alpha <= 0.0 {
                continue;
            }
            let base = index * 4;
            let destination_alpha = out[base + 3];
            let out_alpha = alpha + destination_alpha * (1.0 - alpha);
            if out_alpha <= 0.0 {
                continue;
            }
            for channel in 0..3 {
                let source = pixel[channel] as f32 / 255.0;
                out[base + channel] = (source * alpha
                    + out[base + channel] * destination_alpha * (1.0 - alpha))
                    / out_alpha;
            }
            out[base + 3] = out_alpha;
        }
    }
    out.into_iter()
        .map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect()
}

fn blit(
    source: &[u8],
    source_size: (usize, usize),
    offset: (i32, i32),
    target: &mut [u8],
    target_size: (usize, usize),
) {
    let (source_width, source_height) = source_size;
    let (left, top) = offset;
    let (target_width, target_height) = target_size;
    for y in 0..source_height {
        let ty = top + y as i32;
        if ty < 0 || ty >= target_height as i32 {
            continue;
        }
        for x in 0..source_width {
            let tx = left + x as i32;
            if tx < 0 || tx >= target_width as i32 {
                continue;
            }
            let source_index = (y * source_width + x) * 4;
            let target_index = (ty as usize * target_width + tx as usize) * 4;
            target[target_index..target_index + 4]
                .copy_from_slice(&source[source_index..source_index + 4]);
        }
    }
}
