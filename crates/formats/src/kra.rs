use std::io::{Cursor, Write};
use std::path::Path;

use zip::ZipArchive;

use crate::{
    Error, ExportDocument, ImportedDocument, ImportedLayer, decode_png, error, number, zip_entry,
};

/// Read a `.kra` from disk.
pub fn read_kra(path: &Path) -> Result<ImportedDocument, Error> {
    let bytes =
        std::fs::read(path).map_err(|e| error(format!("reading {}: {e}", path.display())))?;
    read_kra_bytes(&bytes)
}

/// Read a `.kra` from memory; the bytes are a ZIP container.
pub fn read_kra_bytes(bytes: &[u8]) -> Result<ImportedDocument, Error> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| error(format!("opening the Krita archive: {e}")))?;
    let maindoc = zip_entry(&mut zip, &["maindoc.xml".to_string()])
        .ok_or_else(|| error("the Krita archive has no maindoc.xml"))?;
    let xml =
        String::from_utf8(maindoc).map_err(|e| error(format!("maindoc.xml is not UTF-8: {e}")))?;
    let tree =
        roxmltree::Document::parse(&xml).map_err(|e| error(format!("parsing maindoc.xml: {e}")))?;
    let image = tree
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "IMAGE")
        .ok_or_else(|| error("maindoc.xml has no IMAGE element"))?;
    let width: usize = image
        .attribute("width")
        .and_then(number)
        .ok_or_else(|| error("IMAGE width is missing"))?;
    let height: usize = image
        .attribute("height")
        .and_then(number)
        .ok_or_else(|| error("IMAGE height is missing"))?;
    if width == 0 || height == 0 || width > 32_768 || height > 32_768 {
        return Err(error("the Krita canvas size is out of range"));
    }
    let image_name = image.attribute("name").unwrap_or("");
    let colorspace = image.attribute("colorspacename").unwrap_or("RGBA");
    if colorspace != "RGBA" {
        // Krita stores CMYK and float spaces in ways this reader does not decode. Its own merged
        // render is the honest fallback: a flat layer, not a broken one.
        return merged_kra(&mut zip);
    }

    let mut layers = Vec::new();
    if let Some(parent) = image
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "layers")
    {
        for node in parent
            .descendants()
            .filter(|node| node.is_element() && node.tag_name().name() == "layer")
        {
            if node.attribute("nodetype").unwrap_or("paintlayer") != "paintlayer" {
                continue;
            }
            let filename = node.attribute("filename").unwrap_or("");
            if filename.is_empty() {
                continue;
            }
            let name = node.attribute("name").unwrap_or("Layer").to_owned();
            let opacity = node
                .attribute("opacity")
                .and_then(number::<u16>)
                .map_or(1.0, |value| value as f32 / 255.0);
            let visible = node.attribute("visible").is_none_or(|value| value != "0");
            let x = node.attribute("x").and_then(number::<i32>).unwrap_or(0);
            let y = node.attribute("y").and_then(number::<i32>).unwrap_or(0);

            let defaultpixel = zip_entry(
                &mut zip,
                &[
                    format!("{image_name}/layers/{filename}.defaultpixel"),
                    format!("layers/{filename}.defaultpixel"),
                ],
            );
            let data = zip_entry(
                &mut zip,
                &[
                    format!("{image_name}/layers/{filename}"),
                    format!("layers/{filename}"),
                ],
            )
            .ok_or_else(|| error(format!("the Krita layer {filename} is missing")))?;
            let rgba = assemble_tiles(&data, width, height, x, y, defaultpixel.as_deref())?;
            layers.push(ImportedLayer {
                name,
                visible,
                opacity,
                x: 0,
                y: 0,
                width,
                height,
                rgba,
            });
        }
    }

    if layers.is_empty() {
        return merged_kra(&mut zip);
    }
    // maindoc.xml lists the top layer first; the core paints bottom-up.
    layers.reverse();
    Ok(ImportedDocument {
        width,
        height,
        color_space: 0,
        layers,
    })
}

fn merged_kra(zip: &mut ZipArchive<Cursor<&[u8]>>) -> Result<ImportedDocument, Error> {
    let bytes = zip_entry(zip, &["mergedimage.png".to_string()])
        .ok_or_else(|| error("the Krita archive has no raster layer and no mergedimage.png"))?;
    let (png_width, png_height, rgba) = decode_png(&bytes)?;
    Ok(ImportedDocument {
        width: png_width,
        height: png_height,
        color_space: 0,
        layers: vec![ImportedLayer {
            name: "Merged".into(),
            visible: true,
            opacity: 1.0,
            x: 0,
            y: 0,
            width: png_width,
            height: png_height,
            rgba,
        }],
    })
}

fn assemble_tiles(
    file: &[u8],
    canvas_width: usize,
    canvas_height: usize,
    offset_x: i32,
    offset_y: i32,
    defaultpixel: Option<&[u8]>,
) -> Result<Vec<u8>, Error> {
    let (tile_width, tile_height, pixel_size, count, mut cursor) = tile_header(file)?;
    if pixel_size != 4 {
        return Err(error("only 8-bit RGBA Krita layers are supported"));
    }
    let default = match defaultpixel {
        Some([blue, green, red, alpha, ..]) => [*red, *green, *blue, *alpha],
        _ => [0, 0, 0, 0],
    };
    let mut rgba = vec![0_u8; canvas_width * canvas_height * 4];
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&default);
    }
    let expected = tile_width * tile_height * pixel_size;

    for _ in 0..count {
        let line =
            read_line(file, &mut cursor).ok_or_else(|| error("truncated Krita tile header"))?;
        let mut parts = line.split(',');
        let tile_x: i32 = parts
            .next()
            .and_then(number)
            .ok_or_else(|| error("bad tile x"))?;
        let tile_y: i32 = parts
            .next()
            .and_then(number)
            .ok_or_else(|| error("bad tile y"))?;
        let compression = parts.next().unwrap_or("LZF");
        let size: usize = parts
            .next()
            .and_then(number)
            .ok_or_else(|| error("bad tile size"))?;
        if cursor + size > file.len() {
            return Err(error("a Krita tile runs past the end of the layer file"));
        }
        let raw = &file[cursor..cursor + size];
        cursor += size;
        if raw.is_empty() {
            continue;
        }
        let payload = match raw[0] {
            0 => raw[1..].to_vec(),
            1 => lzf::decompress(&raw[1..], expected)
                .map_err(|e| error(format!("decompressing a Krita tile: {e}")))?,
            _ => {
                if compression == "NONE" {
                    raw[1..].to_vec()
                } else {
                    return Err(error("unsupported Krita tile compression"));
                }
            }
        };
        if payload.len() < expected {
            return Err(error("a Krita tile is shorter than its header says"));
        }
        let plane = tile_width * tile_height;
        for y in 0..tile_height {
            for x in 0..tile_width {
                let src = y * tile_width + x;
                let red = payload[2 * plane + src];
                let green = payload[plane + src];
                let blue = payload[src];
                let alpha = payload[3 * plane + src];
                let dx = tile_x + offset_x + x as i32;
                let dy = tile_y + offset_y + y as i32;
                if dx < 0 || dy < 0 || dx >= canvas_width as i32 || dy >= canvas_height as i32 {
                    continue;
                }
                let index = (dy as usize * canvas_width + dx as usize) * 4;
                rgba[index..index + 4].copy_from_slice(&[red, green, blue, alpha]);
            }
        }
    }
    Ok(rgba)
}

fn tile_header(file: &[u8]) -> Result<(usize, usize, usize, usize, usize), Error> {
    let mut cursor = 0;
    let mut tile_width = 64;
    let mut tile_height = 64;
    let mut pixel_size = 4;
    let count;
    loop {
        let line = read_line(file, &mut cursor).ok_or_else(|| error("truncated Krita header"))?;
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("TILEWIDTH") => tile_width = parts.next().and_then(number).unwrap_or(64),
            Some("TILEHEIGHT") => tile_height = parts.next().and_then(number).unwrap_or(64),
            Some("PIXELSIZE") => pixel_size = parts.next().and_then(number).unwrap_or(4),
            Some("DATA") => {
                count = parts.next().and_then(number).unwrap_or(0);
                break;
            }
            _ => {}
        }
    }
    if tile_width == 0 || tile_height == 0 || tile_width > 1024 || tile_height > 1024 {
        return Err(error("the Krita tile size is out of range"));
    }
    Ok((tile_width, tile_height, pixel_size, count, cursor))
}

fn read_line<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a str> {
    let start = *cursor;
    let end = bytes[start..].iter().position(|&byte| byte == b'\n')? + start;
    *cursor = end + 1;
    std::str::from_utf8(&bytes[start..end]).ok()
}

/// Write a Krita `.kra` that Krita can open: ZIP, maindoc.xml, and one tiled LZF layer file per
/// paint layer. The tiled format is the part that matters; a PNG per layer is not a Krita layer.
pub fn write_kra(document: &ExportDocument) -> Result<Vec<u8>, Error> {
    if document.width == 0 || document.height == 0 || document.layers.is_empty() {
        return Err(error("cannot write an empty Krita document"));
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("mimetype", stored)
        .map_err(|e| error(e.to_string()))?;
    zip.write_all(b"application/x-krita")
        .map_err(|e| error(e.to_string()))?;

    let names: Vec<String> = (0..document.layers.len())
        .map(|i| format!("layer{i}"))
        .collect();
    zip.start_file("maindoc.xml", stored)
        .map_err(|e| error(e.to_string()))?;
    zip.write_all(kra_maindoc(document, &names).as_bytes())
        .map_err(|e| error(e.to_string()))?;

    for (index, layer) in document.layers.iter().enumerate() {
        if layer.rgba.len() < document.width * document.height * 4 {
            return Err(error(format!("layer {} is not document-sized", layer.name)));
        }
        zip.start_file(format!("DrawKit/layers/{}", names[index]), stored)
            .map_err(|e| error(e.to_string()))?;
        write_kra_layer(&mut zip, document.width, document.height, &layer.rgba)?;
    }
    let bytes = zip.finish().map_err(|e| error(e.to_string()))?.into_inner();
    Ok(bytes)
}

fn kra_maindoc(document: &ExportDocument, names: &[String]) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <DOC xmlns=\"http://www.calligra.org/DTD/krita\" syntaxVersion=\"2\">\n\
         <IMAGE name=\"DrawKit\" mime=\"application/x-krita\" width=\"{}\" height=\"{}\" \
         colorspacename=\"RGBA\" x-res=\"72\" y-res=\"72\">\n<layers>\n",
        document.width, document.height
    );
    // Krita lists the top layer first; the export document is bottom-up.
    for (index, layer) in document.layers.iter().enumerate().rev() {
        xml.push_str(&format!(
            "<layer name=\"{}\" filename=\"{}\" nodetype=\"paintlayer\" \
             compositeop=\"normal\" colorspacename=\"RGBA\" visible=\"{}\" opacity=\"{}\" \
             x=\"0\" y=\"0\" locked=\"0\" channelflags=\"\"/>\n",
            xml_escape(&layer.name),
            names[index],
            if layer.visible { 1 } else { 0 },
            (layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8
        ));
    }
    xml.push_str("</layers>\n</IMAGE>\n</DOC>\n");
    xml
}

fn write_kra_layer<W: std::io::Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    width: usize,
    height: usize,
    rgba: &[u8],
) -> Result<(), Error> {
    let tile_size = 64;
    let tiles_x = width.div_ceil(tile_size);
    let tiles_y = height.div_ceil(tile_size);
    let count = tiles_x * tiles_y;
    zip.write_all(
        format!(
            "VERSION 2\nTILEWIDTH {tile_size}\nTILEHEIGHT {tile_size}\nPIXELSIZE 4\nDATA {count}\n"
        )
        .as_bytes(),
    )
    .map_err(|e| error(e.to_string()))?;

    let mut planar = vec![0_u8; tile_size * tile_size * 4];
    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            planar.fill(0);
            let plane = tile_size * tile_size;
            for y in 0..tile_size {
                let source_y = tile_y * tile_size + y;
                if source_y >= height {
                    continue;
                }
                for x in 0..tile_size {
                    let source_x = tile_x * tile_size + x;
                    if source_x >= width {
                        continue;
                    }
                    let source = (source_y * width + source_x) * 4;
                    let target = y * tile_size + x;
                    planar[target] = rgba[source + 2];
                    planar[plane + target] = rgba[source + 1];
                    planar[2 * plane + target] = rgba[source];
                    planar[3 * plane + target] = rgba[source + 3];
                }
            }
            let (flag, payload) = match lzf::compress(&planar) {
                Ok(compressed) if compressed.len() < planar.len() => (1_u8, compressed),
                _ => (0_u8, planar.clone()),
            };
            let size = payload.len() + 1;
            zip.write_all(
                format!(
                    "{},{},LZF,{}\n",
                    tile_x * tile_size,
                    tile_y * tile_size,
                    size
                )
                .as_bytes(),
            )
            .map_err(|e| error(e.to_string()))?;
            zip.write_all(&[flag]).map_err(|e| error(e.to_string()))?;
            zip.write_all(&payload).map_err(|e| error(e.to_string()))?;
        }
    }
    Ok(())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
