//! PNG for one layer. Imported and baked layers are stored as their final pixels; the stroke tape
//! can rebuild the layers that were painted from strokes.

use std::io::Cursor;

use crate::surface::{Rect, Surface};

/// The magic every OpenEXR file starts with.
const EXR_MAGIC: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];

/// `wide` writes a float EXR, which is what an extended-range layer uses so highlights above
/// paper white survive the round trip. Eight-bit PNG would clip them, and 16-bit PNG would too.
pub(crate) fn encode_layer(surface: &Surface, rect: Rect, wide: bool) -> Result<Vec<u8>, String> {
    let (width, height) = (rect.width, rect.height);
    if wide {
        let mut pixels = Vec::with_capacity(width * height);
        for y in rect.y..rect.y + height {
            for x in rect.x..rect.x + width {
                pixels.push(surface.get(x, y));
            }
        }
        return drawkit_formats::exr::write_bytes(width, height, &pixels);
    }
    let mut data = Vec::with_capacity(width * height * 4 * if wide { 2 } else { 1 });
    for y in rect.y..rect.y + height {
        for x in rect.x..rect.x + width {
            let pixel = surface.get(x, y);
            let alpha = pixel[3].clamp(0.0, 1.0);
            if alpha <= 0.0 {
                data.extend_from_slice(if wide { &[0; 8] } else { &[0; 4] });
                continue;
            }
            if wide {
                for channel in [pixel[0], pixel[1], pixel[2], alpha] {
                    let value = (channel.clamp(0.0, 1.0) * 65535.0).round() as u16;
                    data.extend_from_slice(&value.to_be_bytes());
                }
            } else {
                for channel in &pixel[..3] {
                    data.push((channel.clamp(0.0, 1.0) * 255.0).round() as u8);
                }
                data.push((alpha * 255.0).round() as u8);
            }
        }
    }
    encode_data(data, width, height, wide)
}

/// A solid fill layer as an image, without allocating a full surface for it.
pub(crate) fn encode_fill(
    fill: crate::color::Rgba,
    width: usize,
    height: usize,
    wide: bool,
) -> Result<Vec<u8>, String> {
    let bytes_per_channel = if wide { 2 } else { 1 };
    let mut data = Vec::with_capacity(width * height * 4 * bytes_per_channel);
    for _ in 0..width * height {
        for channel in fill {
            if wide {
                data.extend_from_slice(
                    &((channel.clamp(0.0, 1.0) * 65535.0).round() as u16).to_be_bytes(),
                );
            } else {
                data.push((channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
    }
    encode_data(data, width, height, wide)
}

fn encode_data(data: Vec<u8>, width: usize, height: usize, wide: bool) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut png), width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(if wide {
            png::BitDepth::Sixteen
        } else {
            png::BitDepth::Eight
        });
        // This runs on a save worker, while the cache is read back on the open path; a slower,
        // smaller PNG is the better trade.
        encoder.set_compression(png::Compression::Default);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(&data)
            .map_err(|error| error.to_string())?;
    }
    Ok(png)
}

pub(crate) fn decode_layer(
    bytes: &[u8],
    width: usize,
    height: usize,
    rect: Rect,
) -> Result<Surface, String> {
    if bytes.starts_with(&EXR_MAGIC) {
        return decode_exr_layer(bytes, width, height, rect);
    }
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let info = reader.info();
    if info.width as usize != rect.width || info.height as usize != rect.height {
        return Err(format!(
            "layer is {}x{}, expected {}x{}",
            info.width, info.height, rect.width, rect.height
        ));
    }
    if info.color_type != png::ColorType::Rgba {
        return Err("layer is not RGBA".to_string());
    }
    let wide = match info.bit_depth {
        png::BitDepth::Eight => false,
        png::BitDepth::Sixteen => true,
        _ => return Err("layer is not 8- or 16-bit".to_string()),
    };
    let bytes_per_channel = if wide { 2 } else { 1 };
    let mut buffer = vec![0_u8; reader.output_buffer_size()];
    reader
        .next_frame(&mut buffer)
        .map_err(|error| error.to_string())?;
    let mut surface = Surface::new(width, height);
    let max = if wide { 65535.0 } else { 255.0 };
    let sample = |index: usize| -> f32 {
        if wide {
            u16::from_be_bytes([buffer[index], buffer[index + 1]]) as f32 / max
        } else {
            buffer[index] as f32 / max
        }
    };
    for y in 0..rect.height {
        for x in 0..rect.width {
            let index = (y * rect.width + x) * 4 * bytes_per_channel;
            let alpha = sample(index + 3 * bytes_per_channel);
            if alpha <= 0.0 {
                continue;
            }
            surface.set(
                rect.x + x,
                rect.y + y,
                [
                    sample(index),
                    sample(index + bytes_per_channel),
                    sample(index + 2 * bytes_per_channel),
                    alpha,
                ],
            );
        }
    }
    Ok(surface)
}

fn decode_exr_layer(
    bytes: &[u8],
    width: usize,
    height: usize,
    rect: Rect,
) -> Result<Surface, String> {
    let (image_width, image_height, pixels) = drawkit_formats::exr::read(bytes)?;
    if image_width != rect.width || image_height != rect.height {
        return Err(format!(
            "layer is {image_width}x{image_height}, expected {}x{}",
            rect.width, rect.height
        ));
    }
    let mut surface = Surface::new(width, height);
    for y in 0..rect.height {
        for x in 0..rect.width {
            let pixel = pixels[y * rect.width + x];
            if pixel[3] <= 0.0 {
                continue;
            }
            surface.set(rect.x + x, rect.y + y, pixel);
        }
    }
    Ok(surface)
}
