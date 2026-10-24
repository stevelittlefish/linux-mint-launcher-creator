mod desktop_entry;
mod scanner;
mod ui;

use std::fs;

use gtk::{glib, prelude::*};

const APP_ID: &str = "io.github.LauncherCreator";
const ICON_SVG: &[u8] = include_bytes!("../assets/launcher-creator.svg");

/// Copies the embedded icon into the user's icon theme so the window, taskbar
/// and any launcher pointing at `Icon=io.github.LauncherCreator` can find it.
fn install_icon() {
    let dir = glib::user_data_dir().join("icons/hicolor/scalable/apps");
    let path = dir.join(format!("{APP_ID}.svg"));
    if fs::read(&path).is_ok_and(|existing| existing == ICON_SVG) {
        return;
    }
    let _ = fs::create_dir_all(&dir).and_then(|_| fs::write(&path, ICON_SVG));
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        install_icon();
        gtk::Window::set_default_icon_name(APP_ID);
    });
    app.connect_activate(ui::build);
    app.run()
}
