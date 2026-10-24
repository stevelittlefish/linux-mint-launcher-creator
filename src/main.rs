mod desktop_entry;
mod scanner;
mod ui;

use gtk::{glib, prelude::*};

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id("io.github.LauncherCreator")
        .build();
    app.connect_activate(ui::build);
    app.run()
}
