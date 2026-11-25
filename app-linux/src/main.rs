// DrawKit for GTK4. Native GTK controls around the same Rust document and GPU compositor the
// other shells use. The first shell keeps the canvas, brush/eraser/fill, colour, size, undo/redo
// and the layer tree; file lifecycle and the zero-copy surface come next.

mod canvas;
mod compositor;

use std::rc::Rc;

use libadwaita as adw;

use adw::prelude::*;
use canvas::{Canvas, Tool};
use gtk::prelude::*;

fn main() -> glib::ExitCode {
    adw::init().expect("libadwaita could not be initialized");
    let app = adw::Application::builder()
        .application_id("dev.drawkit.linux")
        .build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let canvas = Canvas::new();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("DrawKit")
        .default_width(1280)
        .default_height(840)
        .build();
    let header = adw::HeaderBar::new();
    window.set_titlebar(Some(&header));

    let undo = gtk::Button::with_label("Undo");
    let redo = gtk::Button::with_label("Redo");
    {
        let weak = Rc::downgrade(&canvas);
        undo.connect_clicked(move |_| {
            if let Some(canvas) = weak.upgrade() {
                canvas.undo();
            }
        });
    }
    {
        let weak = Rc::downgrade(&canvas);
        redo.connect_clicked(move |_| {
            if let Some(canvas) = weak.upgrade() {
                canvas.redo();
            }
        });
    }
    header.pack_start(&undo);
    header.pack_start(&redo);

    let brush = gtk::ToggleButton::with_label("Brush");
    let eraser = gtk::ToggleButton::with_label("Eraser");
    eraser.set_group(Some(&brush));
    let fill = gtk::ToggleButton::with_label("Fill");
    fill.set_group(Some(&brush));
    brush.set_active(true);
    for (button, tool) in [
        (&brush, Tool::Brush),
        (&eraser, Tool::Eraser),
        (&fill, Tool::Fill),
    ] {
        let weak = Rc::downgrade(&canvas);
        button.connect_toggled(move |button| {
            if button.is_active()
                && let Some(canvas) = weak.upgrade()
            {
                canvas.set_tool(tool);
            }
        });
    }

    let dialog = gtk::ColorDialog::new();
    let colour = gtk::ColorDialogButton::new(Some(dialog));
    colour.set_rgba(&gtk::gdk::RGBA::new(0.10, 0.09, 0.08, 1.0));
    {
        let weak = Rc::downgrade(&canvas);
        colour.connect_rgba_notify(move |button| {
            if let Some(canvas) = weak.upgrade() {
                canvas.set_colour(button.rgba());
            }
        });
    }

    let size = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 120.0, 1.0);
    size.set_value(18.0);
    size.set_draw_value(true);
    size.set_size_request(160, -1);
    {
        let weak = Rc::downgrade(&canvas);
        size.connect_value_changed(move |scale| {
            if let Some(canvas) = weak.upgrade() {
                canvas.set_size(scale.value());
            }
        });
    }

    let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    tools.set_margin_top(6);
    tools.set_margin_bottom(6);
    tools.set_margin_start(10);
    tools.set_margin_end(10);
    tools.append(&undo.clone());
    tools.append(&redo.clone());
    tools.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    tools.append(&brush);
    tools.append(&eraser);
    tools.append(&fill);
    tools.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    tools.append(&gtk::Label::new(Some("Size")));
    tools.append(&size);
    tools.append(&colour);

    let side = gtk::Box::new(gtk::Orientation::Vertical, 6);
    side.set_margin_top(8);
    side.set_margin_bottom(8);
    side.set_margin_start(8);
    side.set_margin_end(8);
    side.set_width_request(260);
    let title = gtk::Label::new(Some("Layers"));
    title.set_xalign(0.0);
    side.append(&title);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&canvas.layer_list()));
    side.append(&scroll);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let add = gtk::Button::with_label("+");
    let group = gtk::Button::with_label("Group");
    let remove = gtk::Button::with_label("−");
    let up = gtk::Button::with_label("↑");
    let down = gtk::Button::with_label("↓");
    {
        let weak = Rc::downgrade(&canvas);
        add.connect_clicked(move |_| {
            if let Some(canvas) = weak.upgrade() {
                canvas.add_layer();
            }
        });
    }
    {
        let weak = Rc::downgrade(&canvas);
        group.connect_clicked(move |_| {
            if let Some(canvas) = weak.upgrade() {
                canvas.add_group();
            }
        });
    }
    {
        let weak = Rc::downgrade(&canvas);
        remove.connect_clicked(move |_| {
            if let Some(canvas) = weak.upgrade() {
                canvas.remove_layer();
            }
        });
    }
    {
        let weak = Rc::downgrade(&canvas);
        up.connect_clicked(move |_| {
            if let Some(canvas) = weak.upgrade() {
                canvas.move_layer(1);
            }
        });
    }
    {
        let weak = Rc::downgrade(&canvas);
        down.connect_clicked(move |_| {
            if let Some(canvas) = weak.upgrade() {
                canvas.move_layer(-1);
            }
        });
    }
    actions.append(&add);
    actions.append(&group);
    actions.append(&remove);
    actions.append(&up);
    actions.append(&down);
    side.append(&actions);
    side.append(&canvas.status_label());

    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&canvas.widget()));
    paned.set_resize_start_child(true);
    paned.set_shrink_start_child(false);
    paned.set_end_child(Some(&side));
    paned.set_position(940);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&tools);
    root.append(&paned);
    window.set_content(Some(&root));
    window.present();
}
