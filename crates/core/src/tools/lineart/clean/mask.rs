//! The binary ink mask and the two pixel passes that do not need endpoints: dust removal and the
//! small binary close.

/// One byte per pixel: zero is background, one is ink. A byte rather than a bit because every pass
/// touches neighbours and the extra cache traffic is not worth the shifts.
pub(super) struct Mask {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) bits: Vec<u8>,
}

impl Mask {
    pub(super) fn from_alpha(alpha: &[f32], width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            bits: alpha
                .iter()
                .map(|value| u8::from(*value >= super::INK_ALPHA))
                .collect(),
        }
    }

    /// Put the binary result back into alpha, keeping the original coverage of surviving ink.
    /// Pixels the cleanup added become fully opaque; pixels it removed become transparent.
    pub(super) fn write_back(&self, alpha: &mut [f32]) {
        for (index, value) in alpha.iter_mut().enumerate() {
            if self.bits[index] == 0 {
                *value = 0.0;
            } else if *value < super::INK_ALPHA {
                *value = 1.0;
            }
        }
    }

    /// Drop every 4-connected component smaller than `min_px`. A visited bitmap keeps the outer
    /// scan linear even when a component is kept.
    pub(super) fn despeckle(&mut self, min_px: usize) {
        if min_px <= 1 {
            return;
        }
        let width = self.width;
        let height = self.height;
        let mut visited = vec![0u8; self.bits.len()];
        let mut component = Vec::new();
        for start in 0..self.bits.len() {
            if self.bits[start] == 0 || visited[start] != 0 {
                continue;
            }
            component.clear();
            visited[start] = 1;
            component.push(start);
            let mut head = 0;
            while head < component.len() {
                let index = component[head];
                head += 1;
                let x = index % width;
                let y = index / width;
                if x > 0 {
                    self.visit(index - 1, &mut component, &mut visited);
                }
                if x + 1 < width {
                    self.visit(index + 1, &mut component, &mut visited);
                }
                if y > 0 {
                    self.visit(index - width, &mut component, &mut visited);
                }
                if y + 1 < height {
                    self.visit(index + width, &mut component, &mut visited);
                }
            }
            if component.len() < min_px {
                for &index in &component {
                    self.bits[index] = 0;
                }
            }
        }
    }

    fn visit(&self, index: usize, component: &mut Vec<usize>, visited: &mut [u8]) {
        if self.bits[index] != 0 && visited[index] == 0 {
            visited[index] = 1;
            component.push(index);
        }
    }

    /// Fill background runs no longer than `gap_px` that sit between two ink pixels on the same
    /// horizontal, vertical or diagonal axis. It reads a snapshot, so filling never snowballs.
    pub(super) fn close_gaps(&mut self, gap_px: usize) {
        if gap_px == 0 {
            return;
        }
        let width = self.width;
        let height = self.height;
        let source = self.bits.clone();
        let axes: [(isize, isize); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];
        for index in 0..source.len() {
            if source[index] == 0 {
                continue;
            }
            let x = (index % width) as isize;
            let y = (index / width) as isize;
            for (dx, dy) in axes {
                let mut step = 1;
                while step <= gap_px + 1 {
                    let nx = x + dx * step as isize;
                    let ny = y + dy * step as isize;
                    if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                        break;
                    }
                    let next = ny as usize * width + nx as usize;
                    if source[next] != 0 {
                        for between in 1..step {
                            let fx = (x + dx * between as isize) as usize;
                            let fy = (y + dy * between as isize) as usize;
                            self.bits[fy * width + fx] = 1;
                        }
                        break;
                    }
                    step += 1;
                }
            }
        }
    }
}
