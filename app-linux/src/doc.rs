// Opening and saving the project. Paths come from GtkFileDialog, which the desktop portal backs
// under Flatpak, or from the command line when a .dkp is double-clicked.

use std::path::Path;

use drawkit_core::tools::Editor;

use crate::compositor::State;

impl State {
    /// Replace the document with the file at `path`. False when the core cannot read it.
    pub(crate) fn open(&mut self, path: &Path) -> bool {
        self.materialize();
        let Some(editor) = Editor::open_file(path) else {
            return false;
        };
        self.width = editor.document.width() as u32;
        self.height = editor.document.height() as u32;
        self.editor = editor;
        self.file = Some(path.to_path_buf());
        // The resident stack holds another document now; the next sync rebuilds it.
        self.surface_layers = 0;
        self.surface_size = (0, 0);
        self.gpu_ready = false;
        self.gpu_stale = false;
        true
    }

    /// Write the project. The resident tiles are read into the CPU cache first, so a save never
    /// misses a stroke that was still only on the GPU.
    pub(crate) fn save(&mut self, path: &Path) -> bool {
        self.materialize();
        if !self.editor.save_tape(path) {
            return false;
        }
        self.file = Some(path.to_path_buf());
        true
    }

    pub(crate) fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }
}
