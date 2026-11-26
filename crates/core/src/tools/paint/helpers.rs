//! Small per-stroke helpers: the tape index a group starts at, the seed, and input smoothing.

use super::*;

impl Editor {
    /// The position of a group's first event on the tape. A group with no events was not recorded -
    /// recording was off, or the stroke never reached `stroke_sample` - so it cannot be replayed
    /// and gets no writer entry.
    pub(crate) fn group_tape_index(&self, group: u64) -> Option<u64> {
        self.recording
            .events()
            .iter()
            .position(|event| event.group == Some(group))
            .map(|index| index as u64)
    }

    /// A seed per stroke, saved with the tape so a replay is the same random stream.
    pub(crate) fn take_seed(&mut self) -> u64 {
        let mut value = self.next_seed;
        self.next_seed = self.next_seed.wrapping_add(0x9E3779B97F4A7C15);
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D049BB133111EB);
        value ^ (value >> 31)
    }

    pub(crate) fn smooth_segment(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
    ) -> ((f32, f32), (f32, f32)) {
        if self.stabilization <= 0.0 {
            return (from, to);
        }
        let step = 1.0 - self.stabilization;
        let previous = self.smoothed.unwrap_or(from);
        let next = (
            previous.0 + (to.0 - previous.0) * step,
            previous.1 + (to.1 - previous.1) * step,
        );
        self.smoothed = Some(next);
        (previous, next)
    }
}
