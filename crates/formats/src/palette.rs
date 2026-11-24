//! Palette interchange. GIMP's `.gpl` is text; Adobe's `.ase` is a small big-endian block file.
//! Both carry the same thing: names and sRGB triples. Only colour entries are read or written.

use std::path::Path;

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteFormat {
    Gpl,
    Ase,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaletteEntry {
    pub name: String,
    /// Straight sRGB in 0..=1.
    pub color: [f32; 3],
}

impl PaletteEntry {
    pub fn new(name: impl Into<String>, color: [f32; 3]) -> Self {
        Self {
            name: name.into(),
            color: [
                color[0].clamp(0.0, 1.0),
                color[1].clamp(0.0, 1.0),
                color[2].clamp(0.0, 1.0),
            ],
        }
    }
}

pub fn read_palette(path: &Path) -> Result<Vec<PaletteEntry>, Error> {
    let bytes = std::fs::read(path)
        .map_err(|error| Error::new(format!("reading {}: {error}", path.display())))?;
    read_palette_bytes(&bytes)
}

pub fn read_palette_bytes(bytes: &[u8]) -> Result<Vec<PaletteEntry>, Error> {
    if bytes.starts_with(b"ASEF") {
        read_ase(bytes)
    } else if bytes.starts_with(b"GIMP Palette") {
        read_gpl(bytes)
    } else {
        Err(Error::new("not a GIMP or Adobe palette"))
    }
}

pub fn write_palette(
    path: &Path,
    entries: &[PaletteEntry],
    format: PaletteFormat,
) -> Result<(), Error> {
    let bytes = match format {
        PaletteFormat::Gpl => write_gpl(entries),
        PaletteFormat::Ase => write_ase(entries),
    };
    std::fs::write(path, bytes)
        .map_err(|error| Error::new(format!("writing {}: {error}", path.display())))
}

pub fn format_for_path(path: &Path) -> Option<PaletteFormat> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "gpl" => Some(PaletteFormat::Gpl),
        "ase" => Some(PaletteFormat::Ase),
        _ => None,
    }
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn read_gpl(bytes: &[u8]) -> Result<Vec<PaletteEntry>, Error> {
    let text = String::from_utf8_lossy(bytes);
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("GIMP Palette")
            || line.starts_with("Name:")
            || line.starts_with("Columns:")
        {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(red), Some(green), Some(blue)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (Ok(red), Ok(green), Ok(blue)) =
            (red.parse::<u8>(), green.parse::<u8>(), blue.parse::<u8>())
        else {
            continue;
        };
        let name = parts.collect::<Vec<_>>().join(" ");
        entries.push(PaletteEntry::new(
            name,
            [
                red as f32 / 255.0,
                green as f32 / 255.0,
                blue as f32 / 255.0,
            ],
        ));
    }
    if entries.is_empty() {
        return Err(Error::new("the palette has no colours"));
    }
    Ok(entries)
}

fn write_gpl(entries: &[PaletteEntry]) -> Vec<u8> {
    let mut out = "GIMP Palette\nName: DrawKit\nColumns: 8\n#\n".to_string();
    for entry in entries {
        let name = if entry.name.trim().is_empty() {
            "Colour".to_string()
        } else {
            entry.name.replace('\n', " ")
        };
        out.push_str(&format!(
            "{:3} {:3} {:3}\t{}\n",
            channel(entry.color[0]),
            channel(entry.color[1]),
            channel(entry.color[2]),
            name
        ));
    }
    out.into_bytes()
}

fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
        *bytes.get(at + 2)?,
        *bytes.get(at + 3)?,
    ]))
}

fn be_f32(bytes: &[u8], at: usize) -> Option<f32> {
    Some(f32::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
        *bytes.get(at + 2)?,
        *bytes.get(at + 3)?,
    ]))
}

fn read_ase(bytes: &[u8]) -> Result<Vec<PaletteEntry>, Error> {
    if bytes.len() < 12 {
        return Err(Error::new("the ASE header is truncated"));
    }
    let count = be_u32(bytes, 8).ok_or_else(|| Error::new("the ASE header is truncated"))?;
    let mut at = 12;
    let mut entries = Vec::new();
    for _ in 0..count {
        let kind = be_u16(bytes, at).ok_or_else(|| Error::new("ASE block truncated"))?;
        let length =
            be_u32(bytes, at + 2).ok_or_else(|| Error::new("ASE block truncated"))? as usize;
        let block = at + 6;
        if block + length > bytes.len() {
            return Err(Error::new("ASE block runs past the file"));
        }
        at = block + length;
        if kind != 0x0001 {
            continue;
        }
        let name_len =
            be_u16(bytes, block).ok_or_else(|| Error::new("ASE name truncated"))? as usize;
        let mut name = String::new();
        for index in 0..name_len.saturating_sub(1) {
            let code = be_u16(bytes, block + 2 + index * 2)
                .ok_or_else(|| Error::new("ASE name truncated"))?;
            if let Some(character) = char::from_u32(code as u32) {
                name.push(character);
            }
        }
        let model = block + 2 + name_len * 2;
        if bytes.get(model..model + 4) != Some(b"RGB ") {
            continue;
        }
        let red = be_f32(bytes, model + 4).ok_or_else(|| Error::new("ASE colour truncated"))?;
        let green = be_f32(bytes, model + 8).ok_or_else(|| Error::new("ASE colour truncated"))?;
        let blue = be_f32(bytes, model + 12).ok_or_else(|| Error::new("ASE colour truncated"))?;
        entries.push(PaletteEntry::new(name, [red, green, blue]));
    }
    if entries.is_empty() {
        return Err(Error::new("the palette has no colours"));
    }
    Ok(entries)
}

fn write_ase(entries: &[PaletteEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + entries.len() * 40);
    out.extend_from_slice(b"ASEF");
    out.extend_from_slice(&1_u16.to_be_bytes());
    out.extend_from_slice(&0_u16.to_be_bytes());
    out.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for (index, entry) in entries.iter().enumerate() {
        let name = if entry.name.trim().is_empty() {
            format!("Colour {}", index + 1)
        } else {
            entry.name.replace('\n', " ")
        };
        let name: Vec<u16> = name.encode_utf16().collect();
        let length = 2 + (name.len() + 1) * 2 + 4 + 12 + 2;
        out.extend_from_slice(&1_u16.to_be_bytes());
        out.extend_from_slice(&(length as u32).to_be_bytes());
        out.extend_from_slice(&((name.len() + 1) as u16).to_be_bytes());
        for unit in &name {
            out.extend_from_slice(&unit.to_be_bytes());
        }
        out.extend_from_slice(&0_u16.to_be_bytes());
        out.extend_from_slice(b"RGB ");
        out.extend_from_slice(&entry.color[0].to_be_bytes());
        out.extend_from_slice(&entry.color[1].to_be_bytes());
        out.extend_from_slice(&entry.color[2].to_be_bytes());
        out.extend_from_slice(&2_u16.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpl_round_trips() {
        let entries = vec![
            PaletteEntry::new("vermilion", [0.77, 0.32, 0.24]),
            PaletteEntry::new("paper", [0.96, 0.94, 0.91]),
        ];
        let bytes = write_gpl(&entries);
        let read = read_gpl(&bytes).expect("a GPL palette");
        assert_eq!(read.len(), 2);
        assert!((read[0].color[0] - entries[0].color[0]).abs() < 0.01);
        assert_eq!(read[1].name, "paper");
    }

    #[test]
    fn ase_round_trips_names_and_colours() {
        let entries = vec![
            PaletteEntry::new("墨", [0.10, 0.09, 0.08]),
            PaletteEntry::new("朱红", [0.77, 0.32, 0.24]),
        ];
        let bytes = write_ase(&entries);
        let read = read_ase(&bytes).expect("an ASE palette");
        assert_eq!(read, entries);
    }

    #[test]
    fn a_foreign_file_is_rejected() {
        assert!(read_palette_bytes(b"not a palette").is_err());
    }
}
