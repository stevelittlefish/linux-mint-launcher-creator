use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk::{gio, pango, prelude::*};

use crate::{
    desktop_entry::DesktopFile,
    scanner::{self, Launcher, Location},
};

/// Freedesktop main categories; the first one found decides the Mint menu section.
const CATEGORIES: [&str; 12] = [
    "(none)", "AudioVideo", "Development", "Education", "Game", "Graphics",
    "Network", "Office", "Science", "Settings", "System", "Utility",
];

const FALLBACK_ICON: &str = "application-x-executable";

/// The launcher open in the form. `path` is `None` until a new launcher is first saved.
struct Editing {
    path: Option<PathBuf>,
    file: DesktopFile,
}

struct Ui {
    window: gtk::ApplicationWindow,
    list: gtk::ListBox,
    search: gtk::SearchEntry,
    show_overrides: gtk::CheckButton,
    name: gtk::Entry,
    comment: gtk::Entry,
    exec: gtk::Entry,
    exec_browse: gtk::Button,
    icon: gtk::Entry,
    icon_browse: gtk::Button,
    icon_preview: gtk::Image,
    workdir: gtk::Entry,
    workdir_browse: gtk::Button,
    category: gtk::DropDown,
    terminal: gtk::Switch,
    no_display: gtk::Switch,
    on_desktop: gtk::CheckButton,
    path_label: gtk::Label,
    new_btn: gtk::Button,
    save_btn: gtk::Button,
    revert_btn: gtk::Button,
    delete_btn: gtk::Button,
    status: gtk::Label,
    launchers: RefCell<Vec<Launcher>>,
    editing: RefCell<Editing>,
}

pub fn build(app: &gtk::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    let ui = Rc::new(Ui::new(app));
    ui.connect_signals();
    ui.refresh(None);
    ui.new_launcher();
    ui.window.present();
}

fn set_icon(image: &gtk::Image, icon: &str) {
    if icon.starts_with('/') && Path::new(icon).exists() {
        image.set_from_file(Some(icon));
    } else if icon.is_empty() || icon.starts_with('/') {
        image.set_icon_name(Some(FALLBACK_ICON));
    } else {
        image.set_icon_name(Some(icon));
    }
}

fn browse_button() -> gtk::Button {
    gtk::Button::builder().icon_name("document-open-symbolic").tooltip_text("Browse…").build()
}

/// Lowercase ASCII slug for a new launcher's filename.
fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() { "launcher".into() } else { slug.into() }
}

/// A free path in the menu dir that doesn't shadow a packaged launcher.
fn new_launcher_path(name: &str) -> PathBuf {
    let dir = scanner::menu_dir();
    let system = scanner::system_ids();
    let slug = slugify(name);
    (1..)
        .map(|n| if n == 1 { format!("{slug}.desktop") } else { format!("{slug}-{n}.desktop") })
        .find(|f| !system.contains(f) && !dir.join(f).exists())
        .map(|f| dir.join(f))
        .unwrap()
}

/// Lets Nemo launch a desktop file without its "untrusted launcher" prompt.
fn mark_trusted(path: &Path) {
    let _ = gio::File::for_path(path).set_attribute_string(
        "metadata::trusted",
        "true",
        gio::FileQueryInfoFlags::NONE,
        gio::Cancellable::NONE,
    );
}

impl Ui {
    fn new(app: &gtk::Application) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("Launcher Creator")
            .default_width(920)
            .default_height(620)
            .build();

        let new_btn = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("New launcher")
            .build();
        let header = gtk::HeaderBar::new();
        header.pack_start(&new_btn);
        window.set_titlebar(Some(&header));

        // Sidebar: search, override toggle, launcher list.
        let search = gtk::SearchEntry::builder().placeholder_text("Search launchers").build();
        let show_overrides = gtk::CheckButton::with_label("Show system overrides");
        show_overrides.set_tooltip_text(Some(
            "Include user copies that modify or hide launchers installed by packages",
        ));
        let list = gtk::ListBox::new();
        list.set_placeholder(Some(
            &gtk::Label::builder().label("No custom launchers yet").margin_top(24).css_classes(["dim-label"]).build(),
        ));
        let scroller = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 6);
        for m in [search.upcast_ref::<gtk::Widget>(), show_overrides.upcast_ref()] {
            m.set_margin_start(8);
            m.set_margin_end(8);
        }
        search.set_margin_top(8);
        sidebar.append(&search);
        sidebar.append(&show_overrides);
        sidebar.append(&scroller);

        // Editor form.
        let icon_preview = gtk::Image::builder().pixel_size(64).icon_name(FALLBACK_ICON).build();
        let name = gtk::Entry::builder().hexpand(true).placeholder_text("Required").build();
        let comment = gtk::Entry::builder().placeholder_text("Tooltip shown in the menu").build();
        let exec = gtk::Entry::builder().placeholder_text("Required, e.g. firefox --private-window").build();
        let icon = gtk::Entry::builder().placeholder_text("Icon name or image path").build();
        let workdir = gtk::Entry::builder().placeholder_text("Optional").build();
        let (exec_browse, icon_browse, workdir_browse) = (browse_button(), browse_button(), browse_button());
        let category = gtk::DropDown::from_strings(&CATEGORIES);
        let terminal = gtk::Switch::builder().halign(gtk::Align::Start).build();
        let no_display = gtk::Switch::builder().halign(gtk::Align::Start).build();
        let on_desktop = gtk::CheckButton::with_label("Also put a shortcut on the Desktop");

        let grid = gtk::Grid::builder().row_spacing(10).column_spacing(12).build();
        let rows: [(&str, &gtk::Widget, Option<&gtk::Button>); 8] = [
            ("Name", name.upcast_ref(), None),
            ("Comment", comment.upcast_ref(), None),
            ("Command", exec.upcast_ref(), Some(&exec_browse)),
            ("Icon", icon.upcast_ref(), Some(&icon_browse)),
            ("Working directory", workdir.upcast_ref(), Some(&workdir_browse)),
            ("Category", category.upcast_ref(), None),
            ("Run in terminal", terminal.upcast_ref(), None),
            ("Hide from menu", no_display.upcast_ref(), None),
        ];
        let row_count = rows.len() as i32;
        for (i, (label, widget, button)) in rows.into_iter().enumerate() {
            let row = i as i32;
            grid.attach(&gtk::Label::builder().label(label).xalign(1.0).build(), 0, row, 1, 1);
            match button {
                Some(button) => {
                    grid.attach(widget, 1, row, 1, 1);
                    grid.attach(button, 2, row, 1, 1);
                }
                None => grid.attach(widget, 1, row, 2, 1),
            }
        }
        grid.attach(&on_desktop, 1, row_count, 2, 1);

        let path_label = gtk::Label::builder()
            .xalign(0.0)
            .selectable(true)
            .wrap(true)
            .css_classes(["dim-label", "caption"])
            .build();
        let status = gtk::Label::builder().xalign(0.0).hexpand(true).ellipsize(pango::EllipsizeMode::End).build();
        let delete_btn = gtk::Button::builder().label("Delete").css_classes(["destructive-action"]).build();
        let revert_btn = gtk::Button::with_label("Revert");
        let save_btn = gtk::Button::builder().label("Save").css_classes(["suggested-action"]).build();
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.append(&delete_btn);
        actions.append(&status);
        actions.append(&revert_btn);
        actions.append(&save_btn);

        let form = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .margin_top(20)
            .margin_bottom(16)
            .margin_start(24)
            .margin_end(24)
            .build();
        form.append(&icon_preview);
        form.append(&grid);
        form.append(&path_label);
        let spacer = gtk::Box::builder().vexpand(true).build();
        form.append(&spacer);
        form.append(&actions);

        let paned = gtk::Paned::builder()
            .start_child(&sidebar)
            .end_child(&gtk::ScrolledWindow::builder().child(&form).build())
            .position(280)
            .shrink_start_child(false)
            .build();
        window.set_child(Some(&paned));

        Self {
            window, list, search, show_overrides, name, comment, exec, exec_browse, icon,
            icon_browse, icon_preview, workdir, workdir_browse, category, terminal, no_display,
            on_desktop, path_label, new_btn, save_btn, revert_btn, delete_btn, status,
            launchers: RefCell::new(Vec::new()),
            editing: RefCell::new(Editing { path: None, file: DesktopFile::new() }),
        }
    }

    fn connect_signals(self: &Rc<Self>) {
        let ui = self.clone();
        self.list.set_filter_func(move |row| {
            let query = ui.search.text().to_lowercase();
            ui.launchers
                .borrow()
                .get(row.index() as usize)
                .is_none_or(|l| l.name.to_lowercase().contains(&query))
        });

        let ui = self.clone();
        self.search.connect_search_changed(move |_| ui.list.invalidate_filter());

        let ui = self.clone();
        self.show_overrides.connect_toggled(move |_| {
            let current = ui.editing.borrow().path.clone();
            ui.refresh(current.as_deref());
        });

        let ui = self.clone();
        self.list.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let path = ui.launchers.borrow().get(row.index() as usize).map(|l| l.path.clone());
            if let Some(path) = path {
                ui.open(&path);
            }
        });

        let ui = self.clone();
        self.icon.connect_changed(move |e| set_icon(&ui.icon_preview, &e.text()));

        let ui = self.clone();
        self.new_btn.connect_clicked(move |_| ui.new_launcher());
        let ui = self.clone();
        self.save_btn.connect_clicked(move |_| ui.save());
        let ui = self.clone();
        self.revert_btn.connect_clicked(move |_| {
            let path = ui.editing.borrow().path.clone();
            match path {
                Some(path) => ui.open(&path),
                None => ui.new_launcher(),
            }
        });
        let ui = self.clone();
        self.delete_btn.connect_clicked(move |_| ui.confirm_delete());

        let ui = self.clone();
        self.exec_browse.connect_clicked(move |_| {
            let ui2 = ui.clone();
            gtk::FileDialog::builder().title("Choose a program").build().open(
                Some(&ui.window),
                gio::Cancellable::NONE,
                move |res| {
                    if let Some(path) = res.ok().and_then(|f| f.path()) {
                        let path = path.to_string_lossy();
                        let quoted = if path.contains(' ') { format!("\"{path}\"") } else { path.into() };
                        ui2.exec.set_text(&quoted);
                    }
                },
            );
        });

        let ui = self.clone();
        self.icon_browse.connect_clicked(move |_| {
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Images"));
            filter.add_mime_type("image/*");
            let ui2 = ui.clone();
            gtk::FileDialog::builder()
                .title("Choose an icon")
                .default_filter(&filter)
                .initial_folder(&gio::File::for_path("/usr/share/icons"))
                .build()
                .open(Some(&ui.window), gio::Cancellable::NONE, move |res| {
                    if let Some(path) = res.ok().and_then(|f| f.path()) {
                        ui2.icon.set_text(&path.to_string_lossy());
                    }
                });
        });

        let ui = self.clone();
        self.workdir_browse.connect_clicked(move |_| {
            let ui2 = ui.clone();
            gtk::FileDialog::builder().title("Choose working directory").build().select_folder(
                Some(&ui.window),
                gio::Cancellable::NONE,
                move |res| {
                    if let Some(path) = res.ok().and_then(|f| f.path()) {
                        ui2.workdir.set_text(&path.to_string_lossy());
                    }
                },
            );
        });
    }

    /// Rescans launchers and rebuilds the list, selecting `select` if present.
    fn refresh(&self, select: Option<&Path>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        *self.launchers.borrow_mut() = scanner::scan(self.show_overrides.is_active());

        let mut selected_row = None;
        for launcher in self.launchers.borrow().iter() {
            let row = launcher_row(launcher);
            self.list.append(&row);
            if select == Some(launcher.path.as_path()) {
                selected_row = Some(row);
            }
        }
        if let Some(row) = selected_row {
            self.list.select_row(Some(&row));
        }
    }

    fn open(&self, path: &Path) {
        match DesktopFile::load(path) {
            Ok(file) => {
                *self.editing.borrow_mut() = Editing { path: Some(path.into()), file };
                self.fill_form();
                self.status.set_text("");
            }
            Err(e) => self.status.set_text(&format!("Couldn't read {}: {e}", path.display())),
        }
    }

    fn new_launcher(&self) {
        *self.editing.borrow_mut() = Editing { path: None, file: DesktopFile::new() };
        self.fill_form();
        self.list.unselect_all();
        self.status.set_text("");
        self.name.grab_focus();
    }

    fn fill_form(&self) {
        let editing = self.editing.borrow();
        let file = &editing.file;
        let get = |key| file.get(key).unwrap_or_default();

        self.name.set_text(&get("Name"));
        self.comment.set_text(&get("Comment"));
        self.exec.set_text(&get("Exec"));
        self.icon.set_text(&get("Icon"));
        set_icon(&self.icon_preview, &get("Icon"));
        self.workdir.set_text(&get("Path"));
        self.terminal.set_active(file.get_bool("Terminal"));
        self.no_display.set_active(file.get_bool("NoDisplay"));

        let categories = get("Categories");
        let selected = categories
            .split(';')
            .find_map(|c| CATEGORIES.iter().skip(1).position(|m| *m == c))
            .map_or(0, |i| i + 1);
        self.category.set_selected(selected as u32);

        // The Desktop checkbox only applies to menu launchers; it mirrors whether a copy exists.
        let in_menu = editing.path.as_ref().is_none_or(|p| p.starts_with(scanner::menu_dir()));
        self.on_desktop.set_visible(in_menu);
        self.on_desktop.set_active(
            editing
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .is_some_and(|n| scanner::desktop_dir().join(n).exists()),
        );

        self.path_label.set_text(&match &editing.path {
            Some(p) => p.display().to_string(),
            None => "New launcher (not saved yet)".into(),
        });
        self.delete_btn.set_sensitive(editing.path.is_some());
    }

    fn save(&self) {
        let name = self.name.text().trim().to_string();
        let exec = self.exec.text().trim().to_string();
        if name.is_empty() || exec.is_empty() {
            self.status.set_text("Name and Command are required.");
            return;
        }

        let mut editing = self.editing.borrow_mut();
        let file = &mut editing.file;
        file.set("Type", "Application");
        file.set("Name", &name);
        file.set("Comment", self.comment.text().trim());
        file.set("Exec", &exec);
        file.set("Icon", self.icon.text().trim());
        file.set("Path", self.workdir.text().trim());
        file.set("Terminal", if self.terminal.is_active() { "true" } else { "false" });
        file.set_flag("NoDisplay", self.no_display.is_active());

        // Swap the main category, keeping any extra ones (e.g. "WebBrowser").
        let mut categories: Vec<String> = file
            .get("Categories")
            .unwrap_or_default()
            .split(';')
            .filter(|c| !c.is_empty() && !CATEGORIES.contains(c))
            .map(String::from)
            .collect();
        let selected = self.category.selected() as usize;
        if selected > 0 {
            categories.insert(0, CATEGORIES[selected].into());
        }
        let categories = if categories.is_empty() { String::new() } else { categories.join(";") + ";" };
        file.set("Categories", &categories);

        let path = editing.path.clone().unwrap_or_else(|| new_launcher_path(&name));
        let result = (|| -> std::io::Result<()> {
            fs::create_dir_all(path.parent().unwrap())?;
            editing.file.save(&path)?;
            if self.on_desktop.is_visible() {
                let copy = scanner::desktop_dir().join(path.file_name().unwrap());
                if self.on_desktop.is_active() {
                    editing.file.save(&copy)?;
                    mark_trusted(&copy);
                } else if copy.exists() {
                    fs::remove_file(&copy)?;
                }
            }
            Ok(())
        })();
        editing.path = Some(path.clone());
        drop(editing);

        match result {
            Ok(()) => {
                self.refresh(Some(&path));
                self.status.set_text("Saved.");
            }
            Err(e) => self.status.set_text(&format!("Save failed: {e}")),
        }
    }

    fn confirm_delete(self: &Rc<Self>) {
        let Some(path) = self.editing.borrow().path.clone() else { return };
        let copy = scanner::desktop_dir().join(path.file_name().unwrap());
        let has_copy = path.starts_with(scanner::menu_dir()) && copy.exists();
        let detail = if has_copy {
            format!("{}\nand its Desktop shortcut will be removed.", path.display())
        } else {
            format!("{} will be removed.", path.display())
        };
        let dialog = gtk::AlertDialog::builder()
            .message(format!("Delete “{}”?", self.name.text()))
            .detail(detail)
            .buttons(["Cancel", "Delete"])
            .cancel_button(0)
            .default_button(0)
            .build();
        let ui = self.clone();
        dialog.choose(Some(&self.window), gio::Cancellable::NONE, move |res| {
            if res != Ok(1) {
                return;
            }
            let result = fs::remove_file(&path).and_then(|_| if has_copy { fs::remove_file(&copy) } else { Ok(()) });
            match result {
                Ok(()) => {
                    ui.refresh(None);
                    ui.new_launcher();
                    ui.status.set_text("Deleted.");
                }
                Err(e) => ui.status.set_text(&format!("Delete failed: {e}")),
            }
        });
    }
}

fn launcher_row(launcher: &Launcher) -> gtk::ListBoxRow {
    let image = gtk::Image::builder().pixel_size(32).build();
    set_icon(&image, &launcher.icon);
    let title = gtk::Label::builder()
        .label(&launcher.name)
        .xalign(0.0)
        .ellipsize(pango::EllipsizeMode::End)
        .build();
    let subtitle = gtk::Label::builder()
        .label(match (launcher.location, launcher.is_override) {
            (Location::Desktop, _) => "Desktop only",
            (Location::Menu, true) => "Menu · overrides system launcher",
            (Location::Menu, false) => "Menu",
        })
        .xalign(0.0)
        .css_classes(["dim-label", "caption"])
        .build();
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.append(&title);
    text.append(&subtitle);
    let hbox = gtk::Box::builder().spacing(10).margin_top(6).margin_bottom(6).margin_start(8).margin_end(8).build();
    hbox.append(&image);
    hbox.append(&text);
    gtk::ListBoxRow::builder().child(&hbox).build()
}
