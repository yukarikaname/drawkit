// DrawKit for GTK4. Native GTK controls around the same Rust document and GPU surface the other
// shells use: the canvas, brush/eraser/fill, colour, size, undo/redo, the layer tree and the
// project file.
//
// The canvas is a GBM buffer: the Rust surface renders into it and GTK scans it out as a dmabuf
// texture. GTK never paints the canvas, so the window keeps its chrome and there is no second
// renderer to fight. See docs/linux-wayland.md.

mod bench;
mod canvas;
mod compositor;
mod dmabuf;
mod doc;
mod edits;
mod ffi;
mod file;
mod fill;
mod layers;
mod layersync;
mod paint;
mod pixels;

// The C entry points live in drawkit-gpu; naming the crate keeps them linked.
use drawkit_gpu as _;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use libadwaita as adw;

use adw::prelude::*;
use canvas::{Canvas, Tool};
use gtk::gio::prelude::*;
use gtk::prelude::*;

thread_local! {
    /// The one document. Both `activate` (no file) and `open` (a double-clicked file) run through
    /// `show`, and a second launch of an already running instance only adds a file.
    static SHELL: RefCell<Option<Rc<Canvas>>> = const { RefCell::new(None) };
}

/// The proof palette as GTK CSS. libadwaita keeps its layout and behaviour; only the colour and
/// the corners change, so the shell still reads as a GNOME app. The selected tool is paper, the
/// chrome is slate bounded by hairlines, and nothing in the chrome carries a hue: the painting is
/// the only coloured thing in the window.
fn install_theme() {
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        "window, .background { background-color: #14171A; }
         headerbar { background: #1D2126; box-shadow: none; border-bottom: 1px solid #2E343A; }
         headerbar button { border-radius: 2px; }
         button { border-radius: 2px; }
         togglebutton { border-radius: 2px; }
         togglebutton:checked { background: #F4F1EA; color: #14171A; }
         togglebutton:checked:hover { background: #ECEAE4; }
         separator { background: #2E343A; }
         scale trough { background: #2E343A; border-radius: 1px; }
         scale highlight { background: #ECEAE4; }
         scale slider { background: #F4F1EA; border: 1px solid #2E343A; }
         label { color: #ECEAE4; }
         label.dim-label, .dim-label { color: #8D959D; }
",
    );
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn main() -> glib::ExitCode {
    adw::init().expect("libadwaita could not be initialized");
    install_theme();
    // GTK picks the layout direction from the locale. The override exists so a translation review
    // or a screenshot run can force either direction without changing the session language.
    if let Ok(direction) = std::env::var("DRAWKIT_LAYOUT_DIRECTION") {
        match direction.to_ascii_lowercase().as_str() {
            "rtl" => gtk::Widget::set_default_direction(gtk::TextDirection::Rtl),
            "ltr" => gtk::Widget::set_default_direction(gtk::TextDirection::Ltr),
            _ => {}
        }
    }
    let app = adw::Application::builder()
        .application_id("dev.drawkit.DrawKit")
        .flags(gtk::gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_activate(|app| show(app, None));
    app.connect_open(|app, files, _| {
        show(app, files.first().and_then(|file| file.path()));
    });
    app.run()
}

fn show(app: &adw::Application, path: Option<PathBuf>) {
    if let Some(canvas) = SHELL.with(|shell| shell.borrow().clone()) {
        if let Some(path) = path
            && let Some(window) = app.active_window()
            && canvas.open_path(&path)
        {
            file::sync_title(&window, &canvas);
        }
        if let Some(window) = app.active_window() {
            window.present();
        }
        return;
    }
    let canvas = build_ui(app);
    SHELL.with(|shell| *shell.borrow_mut() = Some(canvas.clone()));
    if let Some(path) = path
        && let Some(window) = app.active_window()
        && canvas.open_path(&path)
    {
        file::sync_title(&window, &canvas);
    }
}

fn build_ui(app: &adw::Application) -> Rc<Canvas> {
    let canvas = Canvas::new();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("DrawKit")
        .default_width(1280)
        .default_height(840)
        .build();
    // An AdwApplicationWindow owns its titlebar, so the header goes in an AdwToolbarView instead
    // of through set_titlebar (which aborts the process for this window class).
    let header = adw::HeaderBar::new();

    install_file_actions(app, &window, &canvas);

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
    let open = gtk::Button::with_label("Open");
    open.set_action_name(Some("win.open"));
    let save = gtk::Button::with_label("Save");
    save.set_action_name(Some("win.save"));
    header.pack_start(&undo);
    header.pack_start(&redo);
    header.pack_end(&save);
    header.pack_end(&open);

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

    // append() and margins in start/end mirror with the window direction; nothing here is placed
    // by a physical left or right.
    let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    tools.set_margin_top(6);
    tools.set_margin_bottom(6);
    tools.set_margin_start(10);
    tools.set_margin_end(10);
    // Undo and redo live in the header bar; a widget can only have one parent.
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

    let layer_opacity = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    layer_opacity.set_value(canvas.active_opacity() * 100.0);
    layer_opacity.set_draw_value(true);
    layer_opacity.set_size_request(160, -1);
    let syncing = Rc::new(std::cell::Cell::new(false));
    {
        let weak = Rc::downgrade(&canvas);
        let syncing = syncing.clone();
        layer_opacity.connect_value_changed(move |scale| {
            if syncing.get() {
                return;
            }
            if let Some(canvas) = weak.upgrade() {
                canvas.set_layer_opacity(scale.value() / 100.0);
            }
        });
    }
    {
        let weak = Rc::downgrade(&canvas);
        let syncing = syncing.clone();
        let slider = layer_opacity.clone();
        canvas.layer_list().connect_row_selected(move |_, row| {
            if row.is_none() {
                return;
            }
            if let Some(canvas) = weak.upgrade() {
                syncing.set(true);
                slider.set_value(canvas.active_opacity() * 100.0);
                syncing.set(false);
            }
        });
    }
    let opacity_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    opacity_row.append(&gtk::Label::new(Some("Opacity")));
    opacity_row.append(&layer_opacity);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&canvas.layer_list()));
    side.append(&scroll);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let add = gtk::Button::with_label("+");
    let group = gtk::Button::with_label("Group");
    let lines = gtk::Button::with_label("Line art");
    let remove = gtk::Button::with_label("−");
    let up = gtk::Button::with_label("↑");
    let down = gtk::Button::with_label("↓");
    for (button, action) in [
        (&add, 0),
        (&group, 1),
        (&lines, 2),
        (&remove, 3),
        (&up, 4),
        (&down, 5),
    ] {
        let weak = Rc::downgrade(&canvas);
        button.connect_clicked(move |_| {
            if let Some(canvas) = weak.upgrade() {
                match action {
                    0 => canvas.add_layer(),
                    1 => canvas.add_group(),
                    2 => canvas.extract_line_art(),
                    3 => canvas.remove_layer(),
                    4 => canvas.move_layer(1),
                    _ => canvas.move_layer(-1),
                }
            }
        });
    }
    actions.append(&add);
    actions.append(&group);
    actions.append(&lines);
    actions.append(&remove);
    actions.append(&up);
    actions.append(&down);
    side.append(&opacity_row);
    side.append(&actions);
    side.append(&canvas.status_label());

    // A paned would be nicer, but it positions its split from the start child's natural width and
    // the canvas answers "one pixel" until it has a paintable: the drawing area came out 1 px wide
    // and nothing was visible. A box lets the canvas take whatever the side column does not, and
    // it mirrors with the window direction.
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let canvas_widget = canvas.widget();
    canvas_widget.set_hexpand(true);
    body.append(&canvas_widget);
    body.append(&side);

    {
        // The Wayland surface exists only once the window is realized.
        let weak = Rc::downgrade(&canvas);
        window.connect_realize(move |window| {
            if let Some(canvas) = weak.upgrade() {
                canvas.attach(window.upcast_ref());
            }
        });
    }

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.append(&tools);
    root.append(&body);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&root));
    window.set_content(Some(&view));
    window.present();
    if std::env::var("DRAWKIT_BENCH_POINTER").is_ok_and(|value| value == "1") {
        let weak = Rc::downgrade(&canvas);
        glib::timeout_add_seconds_local_once(1, move || {
            if let Some(canvas) = weak.upgrade() {
                bench::run(&canvas);
            }
        });
    }
    canvas
}

fn install_file_actions(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    canvas: &Rc<Canvas>,
) {
    let open = gtk::gio::SimpleAction::new("open", None);
    {
        let window = adw::ApplicationWindow::clone(window);
        let canvas = Rc::clone(canvas);
        open.connect_activate(move |_, _| file::open(window.upcast_ref(), &canvas));
    }
    window.add_action(&open);
    app.set_accels_for_action("win.open", &["<Primary>o"]);

    let save = gtk::gio::SimpleAction::new("save", None);
    {
        let window = adw::ApplicationWindow::clone(window);
        let canvas = Rc::clone(canvas);
        save.connect_activate(move |_, _| file::save(window.upcast_ref(), &canvas, false));
    }
    window.add_action(&save);
    app.set_accels_for_action("win.save", &["<Primary>s"]);

    let save_as = gtk::gio::SimpleAction::new("save-as", None);
    {
        let window = adw::ApplicationWindow::clone(window);
        let canvas = Rc::clone(canvas);
        save_as.connect_activate(move |_, _| file::save(window.upcast_ref(), &canvas, true));
    }
    window.add_action(&save_as);
    app.set_accels_for_action("win.save-as", &["<Primary><Shift>s"]);
}
