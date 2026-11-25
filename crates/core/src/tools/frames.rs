//! Animation frames as whole-document snapshots.
//!
//! A frame owns the layer tree, its own undo stack and its own tape. The active frame lives in the
//! editor's `document`, `history` and `recording` fields; the other slots in `frames` are `Some`.
//! Switching swaps the active state with a slot, so every existing tool keeps working unchanged.
//!
//! Saving all frames into `.dkp` is a format change and is not part of this step; the active frame
//! is still the one written by the existing tape writer.

use super::*;

#[derive(Clone)]
pub(super) struct Frame {
    pub(super) document: Document,
    pub(super) history: History,
    pub(super) recording: Recording,
    pub(super) time_ms: u64,
}

impl Frame {
    fn blank(source: &Document) -> Self {
        let mut document = Document::new(source.width(), source.height());
        document.color_space = source.color_space;
        document.dynamic_range = source.dynamic_range;
        Self {
            document,
            history: History::new(256 << 20),
            recording: Recording::default(),
            time_ms: 0,
        }
    }
}

impl Editor {
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn active_frame(&self) -> usize {
        self.active_frame
    }

    /// Add a blank or duplicated frame after the active one and select it.
    pub fn add_frame(&mut self, duplicate: bool) -> usize {
        self.finish_fill_animation();
        let frame = if duplicate {
            Frame {
                document: self.document.clone(),
                history: self.history.clone(),
                recording: self.recording.clone(),
                time_ms: self.time_ms,
            }
        } else {
            Frame::blank(&self.document)
        };
        let index = (self.active_frame + 1).min(self.frames.len());
        self.frames.insert(index, Some(frame));
        self.select_frame(index);
        index
    }

    /// Select a frame. The selection and any in-progress fill belong to the frame being left.
    pub fn select_frame(&mut self, index: usize) -> bool {
        if index >= self.frames.len() || index == self.active_frame {
            return false;
        }
        self.finish_fill_animation();
        self.cancel_prediction();
        self.selection = None;
        let current = Frame {
            document: std::mem::replace(&mut self.document, Document::new(1, 1)),
            history: std::mem::replace(&mut self.history, History::new(256 << 20)),
            recording: std::mem::take(&mut self.recording),
            time_ms: self.time_ms,
        };
        self.frames[self.active_frame] = Some(current);
        let next = self.frames[index]
            .take()
            .unwrap_or_else(|| Frame::blank(&self.document));
        self.document = next.document;
        self.history = next.history;
        self.recording = next.recording;
        self.time_ms = next.time_ms;
        self.active_frame = index;
        true
    }

    pub fn remove_frame(&mut self, index: usize) -> bool {
        if self.frames.len() <= 1 || index >= self.frames.len() {
            return false;
        }
        self.finish_fill_animation();
        if index == self.active_frame {
            self.frames.remove(index);
            let next = index.min(self.frames.len() - 1);
            self.active_frame = next;
            let loaded = self.frames[next]
                .take()
                .unwrap_or_else(|| Frame::blank(&self.document));
            self.document = loaded.document;
            self.history = loaded.history;
            self.recording = loaded.recording;
            self.time_ms = loaded.time_ms;
            self.selection = None;
        } else {
            self.frames.remove(index);
            if index < self.active_frame {
                self.active_frame -= 1;
            }
        }
        true
    }

    /// Composite the frames around the active one into transparent RGBA8. Previous frames are
    /// red, next frames are green; the nearest frame is the strongest.
    pub fn onion_image(&self, before: usize, after: usize, out: &mut [u8]) -> bool {
        let width = self.document.width();
        let height = self.document.height();
        if width == 0 || height == 0 || out.len() < width * height * 4 {
            return false;
        }
        out.fill(0);
        let rect = Rect::new(0, 0, width, height);
        for (offset, red) in (1..=before).map(|offset| (offset, true)) {
            if offset > self.active_frame {
                break;
            }
            let Some(frame) = self.frames[self.active_frame - offset].as_ref() else {
                continue;
            };
            let color = if red {
                [1.0, 0.35, 0.35, 1.0]
            } else {
                [0.35, 1.0, 0.45, 1.0]
            };
            let alpha = 0.35 / offset as f32;
            let pixels = frame.document.composite_rect(rect);
            blend_tinted(out, &pixels, width, color, alpha);
        }
        for offset in 1..=after {
            let index = self.active_frame + offset;
            if index >= self.frames.len() {
                break;
            }
            let Some(frame) = self.frames[index].as_ref() else {
                continue;
            };
            let color = [0.35, 1.0, 0.45, 1.0];
            let alpha = 0.35 / offset as f32;
            let pixels = frame.document.composite_rect(rect);
            blend_tinted(out, &pixels, width, color, alpha);
        }
        true
    }

    /// Composite one frame's canvas into RGBA8 for a thumbnail.
    pub fn frame_image(&self, index: usize, out: &mut [u8]) -> bool {
        let width = self.document.width();
        let height = self.document.height();
        if out.len() < width * height * 4 {
            return false;
        }
        if index == self.active_frame {
            let pixels = self.document.composite_rect(Rect::new(0, 0, width, height));
            write_rgba(out, &pixels);
            return true;
        }
        let Some(frame) = self.frames.get(index).and_then(Option::as_ref) else {
            return false;
        };
        let pixels = frame
            .document
            .composite_rect(Rect::new(0, 0, width, height));
        write_rgba(out, &pixels);
        true
    }
}

fn blend_tinted(out: &mut [u8], pixels: &[Rgba], width: usize, color: Rgba, alpha: f32) {
    for (index, pixel) in pixels.iter().enumerate() {
        let coverage = pixel[3].clamp(0.0, 1.0) * alpha;
        if coverage <= 0.0 {
            continue;
        }
        let base = index * 4;
        let dst_alpha = out[base + 3] as f32 / 255.0;
        let dst = if dst_alpha > 0.0 {
            [
                out[base] as f32 / 255.0 / dst_alpha,
                out[base + 1] as f32 / 255.0 / dst_alpha,
                out[base + 2] as f32 / 255.0 / dst_alpha,
                dst_alpha,
            ]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        };
        let src = [color[0], color[1], color[2], coverage];
        let mixed = over(src, dst);
        let alpha = mixed[3].clamp(0.0, 1.0);
        out[base] = (mixed[0].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        out[base + 1] = (mixed[1].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        out[base + 2] = (mixed[2].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        out[base + 3] = (alpha * 255.0).round() as u8;
    }
    let _ = width;
}

fn write_rgba(out: &mut [u8], pixels: &[Rgba]) {
    for (index, pixel) in pixels.iter().enumerate() {
        let base = index * 4;
        let alpha = pixel[3].clamp(0.0, 1.0);
        out[base] = (pixel[0].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        out[base + 1] = (pixel[1].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        out[base + 2] = (pixel[2].clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
        out[base + 3] = (alpha * 255.0).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_keep_their_own_pixels() {
        let mut editor = Editor::new(16, 16, 1 << 20);
        editor.document.layers[0]
            .surface
            .set(1, 1, [1.0, 0.0, 0.0, 1.0]);
        let second = editor.add_frame(true);
        assert_eq!(editor.frame_count(), 2);
        assert_eq!(editor.active_frame(), second);
        editor.document.layers[0]
            .surface
            .set(2, 2, [0.0, 1.0, 0.0, 1.0]);
        assert!(editor.select_frame(0));
        assert_eq!(editor.document.layers[0].surface.get(1, 1)[0], 1.0);
        assert_eq!(editor.document.layers[0].surface.get(2, 2)[3], 0.0);
        assert!(editor.select_frame(second));
        assert_eq!(editor.document.layers[0].surface.get(2, 2)[1], 1.0);
    }

    #[test]
    fn onion_contains_a_neighbour_frame() {
        let mut editor = Editor::new(8, 8, 1 << 20);
        editor.document.layers[0]
            .surface
            .set(4, 4, [0.0, 0.0, 1.0, 1.0]);
        editor.add_frame(true);
        editor.document.layers[0]
            .surface
            .set(4, 4, [0.0, 0.0, 0.0, 0.0]);
        let mut out = vec![0u8; 8 * 8 * 4];
        assert!(editor.onion_image(1, 0, &mut out));
        let index = (4 * 8 + 4) * 4;
        assert!(out[index + 3] > 0, "the previous frame was not composited");
    }

    #[test]
    fn the_last_frame_cannot_be_removed() {
        let mut editor = Editor::new(8, 8, 1 << 20);
        assert!(!editor.remove_frame(0));
    }
}
