// The layer list. GtkBox and GtkListBox mirror in a right-to-left window, so the eye button and
// the name swap sides without extra code; nothing here is placed by hand.

use std::rc::Rc;

use gtk::prelude::*;

use crate::canvas::Canvas;

impl Canvas {
    pub(crate) fn refresh_layers(self: &Rc<Self>) {
        while let Some(child) = self.layers.first_child() {
            self.layers.remove(&child);
        }
        let (count, active) = self.layer_len();
        for index in (0..count).rev() {
            let Some((name, visible, group)) = self.layer_name(index) else {
                continue;
            };
            let row = gtk::ListBoxRow::new();
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let eye = gtk::ToggleButton::with_label(if visible { "◉" } else { "○" });
            eye.set_active(visible);
            let weak = Rc::downgrade(self);
            eye.connect_toggled(move |button| {
                if let Some(canvas) = weak.upgrade() {
                    canvas.set_visible(index, button.is_active());
                }
            });
            let text = format!("{}{}", if group { "▸ " } else { "" }, name);
            let label = gtk::Label::new(Some(&text));
            label.set_xalign(0.0);
            hbox.append(&eye);
            hbox.append(&label);
            row.set_child(Some(&hbox));
            self.layers.append(&row);
            if index == active {
                self.layers.select_row(Some(&row));
            }
        }
    }
}
