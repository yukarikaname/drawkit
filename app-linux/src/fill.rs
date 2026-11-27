// Fills go straight into the resident tiles: one mask, one GPU pass, one undo step. There is no
// CPU fill path, because the CPU surface is only the history cache and the canvas shows the GPU
// pixels.

use std::rc::Rc;

use crate::canvas::Canvas;

impl Canvas {
    pub(crate) fn begin_fill(self: &Rc<Self>, x: usize, y: usize) -> bool {
        let started = self.state.borrow_mut().gpu_fill(x, y);
        self.update_status();
        if started {
            self.schedule();
        }
        started
    }
}
