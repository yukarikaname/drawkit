use super::*;

pub(super) fn encode_events(document: &TapeDocument) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(64 + document.events.len() * 48);
    out.extend_from_slice(MAGIC_V3);
    put_u32(&mut out, document.events.len() as u32);
    for event in &document.events {
        put_u64(&mut out, event.time_ms);
        match event.group {
            Some(group) => {
                out.push(1);
                put_u64(&mut out, group);
            }
            None => out.push(0),
        }
        match event.command {
            TapeCommand::Stroke {
                layer,
                from,
                to,
                radius,
                pressure,
                color,
                brush,
                seed,
                tilt,
                azimuth,
                roll,
                scale,
            } => {
                out.push(1);
                out.push(brush);
                out.push(scale);
                put_u32(&mut out, layer);
                put_point(&mut out, from);
                put_point(&mut out, to);
                put_f32(&mut out, radius);
                put_f32(&mut out, pressure);
                put_color(&mut out, color);
                put_u64(&mut out, seed);
                put_f32(&mut out, tilt[0]);
                put_f32(&mut out, tilt[1]);
                put_f32(&mut out, azimuth);
                put_f32(&mut out, roll);
            }
            TapeCommand::Erase {
                layer,
                from,
                to,
                radius,
                pressure,
                brush,
                seed,
                tilt,
                azimuth,
                roll,
                scale,
            } => {
                out.push(2);
                out.push(brush);
                out.push(scale);
                put_u32(&mut out, layer);
                put_point(&mut out, from);
                put_point(&mut out, to);
                put_f32(&mut out, radius);
                put_f32(&mut out, pressure);
                put_u64(&mut out, seed);
                put_f32(&mut out, tilt[0]);
                put_f32(&mut out, tilt[1]);
                put_f32(&mut out, azimuth);
                put_f32(&mut out, roll);
            }
            TapeCommand::Fill {
                layer,
                x,
                y,
                tolerance,
                color,
                close,
                expand,
                feather,
            } => {
                out.push(3);
                put_u32(&mut out, layer);
                put_u32(&mut out, x);
                put_u32(&mut out, y);
                put_f32(&mut out, tolerance);
                put_color(&mut out, color);
                put_u32(&mut out, close);
                put_u32(&mut out, expand);
                put_u32(&mut out, feather);
            }
            TapeCommand::Smudge {
                layer,
                from,
                to,
                radius,
                pressure,
                strength,
                brush,
            } => {
                out.push(5);
                out.push(brush);
                put_u32(&mut out, layer);
                put_point(&mut out, from);
                put_point(&mut out, to);
                put_f32(&mut out, radius);
                put_f32(&mut out, pressure);
                put_f32(&mut out, strength);
            }
            TapeCommand::Clear { layer, color } => {
                out.push(4);
                put_u32(&mut out, layer);
                put_color(&mut out, color);
            }
        }
    }
    Ok(out)
}

pub(super) fn decode_events(data: &[u8], version: u64) -> Result<Vec<TapeEvent>, Error> {
    let mut cursor = Reader { data, at: 0 };
    let magic = if version >= 5 {
        MAGIC_V3
    } else if version >= 2 {
        MAGIC_V2
    } else {
        MAGIC
    };
    if cursor.take(4)? != magic.as_slice() {
        return Err(error("the stroke stream has the wrong magic"));
    }
    let count = cursor.u32()? as usize;
    if count > MAX_EVENTS {
        return Err(error("the stroke stream has too many events"));
    }
    let mut events = Vec::with_capacity(count.min(65_536));
    for _ in 0..count {
        let time_ms = cursor.u64()?;
        let group = match cursor.u8()? {
            0 => None,
            _ => Some(cursor.u64()?),
        };
        let command = match cursor.u8()? {
            1 => TapeCommand::Stroke {
                brush: if version >= 2 { cursor.u8()? } else { 0 },
                scale: if version >= 5 { cursor.u8()? } else { 0 },
                layer: cursor.u32()?,
                from: cursor.point()?,
                to: cursor.point()?,
                radius: cursor.f32()?,
                pressure: cursor.f32()?,
                color: cursor.color()?,
                seed: cursor.u64()?,
                tilt: [cursor.f32()?, cursor.f32()?],
                azimuth: cursor.f32()?,
                roll: cursor.f32()?,
            },
            2 => TapeCommand::Erase {
                brush: if version >= 2 { cursor.u8()? } else { 0 },
                scale: if version >= 5 { cursor.u8()? } else { 0 },
                layer: cursor.u32()?,
                from: cursor.point()?,
                to: cursor.point()?,
                radius: cursor.f32()?,
                pressure: cursor.f32()?,
                seed: cursor.u64()?,
                tilt: [cursor.f32()?, cursor.f32()?],
                azimuth: cursor.f32()?,
                roll: cursor.f32()?,
            },
            3 => TapeCommand::Fill {
                layer: cursor.u32()?,
                x: cursor.u32()?,
                y: cursor.u32()?,
                tolerance: cursor.f32()?,
                color: cursor.color()?,
                close: cursor.u32()?,
                expand: cursor.u32()?,
                feather: cursor.u32()?,
            },
            5 => TapeCommand::Smudge {
                brush: cursor.u8()?,
                layer: cursor.u32()?,
                from: cursor.point()?,
                to: cursor.point()?,
                radius: cursor.f32()?,
                pressure: cursor.f32()?,
                strength: cursor.f32()?,
            },
            4 => TapeCommand::Clear {
                layer: cursor.u32()?,
                color: cursor.color()?,
            },
            other => return Err(error(format!("unknown tape command {other}"))),
        };
        events.push(TapeEvent {
            time_ms,
            group,
            command,
        });
    }
    Ok(events)
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let end = self
            .at
            .checked_add(length)
            .ok_or_else(|| error("tape offset overflow"))?;
        if end > self.data.len() {
            return Err(error("the stroke stream is truncated"));
        }
        let slice = &self.data[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let bytes: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, Error> {
        let bytes: [u8; 8] = self.take(8)?.try_into().unwrap();
        Ok(u64::from_le_bytes(bytes))
    }

    fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn point(&mut self) -> Result<[f32; 2], Error> {
        Ok([self.f32()?, self.f32()?])
    }

    fn color(&mut self) -> Result<[f32; 4], Error> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_point(out: &mut Vec<u8>, point: [f32; 2]) {
    put_f32(out, point[0]);
    put_f32(out, point[1]);
}

fn put_color(out: &mut Vec<u8>, color: [f32; 4]) {
    for channel in color {
        put_f32(out, channel);
    }
}
