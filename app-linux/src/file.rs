// File open/save through GtkFileDialog. The dialog is a portal client under Flatpak, so the shell
// needs no home-directory permission to read a project the user picked.

use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;

use crate::canvas::Canvas;

fn project_filter() -> gtk::FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("DrawKit project"));
    filter.add_suffix("dkp");
    filter
}

fn document_filters() -> gtk::gio::ListStore {
    let images = gtk::FileFilter::new();
    images.set_name(Some("Images and other documents"));
    for suffix in ["kra", "psd", "png", "jpg", "jpeg", "webp"] {
        images.add_suffix(suffix);
    }
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&project_filter());
    filters.append(&images);
    filters
}

fn save_filters() -> gtk::gio::ListStore {
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&project_filter());
    filters
}

fn home_folder() -> Option<gtk::gio::File> {
    let home = std::env::var_os("HOME")?;
    Some(gtk::gio::File::for_path(PathBuf::from(home)))
}

fn report(window: &gtk::Window, message: &str) {
    let dialog = gtk::AlertDialog::builder()
        .message(message)
        .modal(true)
        .build();
    dialog.show(Some(window));
}

/// Keep the window title on the document name. Called after every open and save.
pub(crate) fn sync_title(window: &gtk::Window, canvas: &Canvas) {
    let title = canvas
        .file_name()
        .map_or_else(|| "DrawKit".to_string(), |name| format!("{name} — DrawKit"));
    window.set_title(Some(&title));
}

pub(crate) fn open(window: &gtk::Window, canvas: &Rc<Canvas>) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Open painting");
    dialog.set_filters(Some(&document_filters()));
    dialog.set_default_filter(Some(&project_filter()));
    if let Some(folder) = home_folder() {
        dialog.set_initial_folder(Some(&folder));
    }
    let window = gtk::Window::clone(window);
    let parent = gtk::Window::clone(&window);
    let weak = Rc::downgrade(canvas);
    dialog.open(
        Some(&parent),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                return;
            };
            let Some(canvas) = weak.upgrade() else {
                return;
            };
            if canvas.open_path(&path) {
                sync_title(&window, &canvas);
            } else {
                report(&window, "That file could not be opened.");
            }
        },
    );
}

pub(crate) fn save(window: &gtk::Window, canvas: &Rc<Canvas>, ask: bool) {
    if !ask && canvas.has_file() {
        if !canvas.save() {
            report(window, "The painting could not be saved.");
        }
        return;
    }
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Save painting");
    dialog.set_filters(Some(&save_filters()));
    dialog.set_default_filter(Some(&project_filter()));
    dialog.set_initial_name(Some("Untitled.dkp"));
    if let Some(folder) = home_folder() {
        dialog.set_initial_folder(Some(&folder));
    }
    let window = gtk::Window::clone(window);
    let parent = gtk::Window::clone(&window);
    let weak = Rc::downgrade(canvas);
    dialog.save(
        Some(&parent),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path().map(project_path) else {
                return;
            };
            let Some(canvas) = weak.upgrade() else {
                return;
            };
            if canvas.save_to(&path) {
                sync_title(&window, &canvas);
            } else {
                report(&window, "The painting could not be saved.");
            }
        },
    );
}

/// The save dialog returns whatever the user typed; the project format is ours to append.
fn project_path(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        return path;
    }
    path.with_extension("dkp")
}
