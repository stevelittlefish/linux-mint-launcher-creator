//! Finds the user's own launchers, skipping anything a package installed.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use gtk::glib;

use crate::desktop_entry::DesktopFile;

#[derive(Clone, Copy, PartialEq)]
pub enum Location {
    Menu,
    Desktop,
}

pub struct Launcher {
    pub path: PathBuf,
    pub name: String,
    pub icon: String,
    pub location: Location,
    /// A user copy that shadows or hides a packaged launcher, e.g. made by the menu editor.
    pub is_override: bool,
}

/// `~/.local/share/applications`, where menu launchers live.
pub fn menu_dir() -> PathBuf {
    glib::user_data_dir().join("applications")
}

pub fn desktop_dir() -> PathBuf {
    glib::user_special_dir(glib::UserDirectory::Desktop)
        .unwrap_or_else(|| glib::home_dir().join("Desktop"))
}

/// Desktop-file IDs installed by packages, Flatpak or Snap.
pub fn system_ids() -> HashSet<String> {
    let mut dirs: Vec<PathBuf> = glib::system_data_dirs()
        .into_iter()
        .map(|d| d.join("applications"))
        .collect();
    dirs.push("/var/lib/flatpak/exports/share/applications".into());
    dirs.push(glib::user_data_dir().join("flatpak/exports/share/applications"));
    dirs.push("/var/lib/snapd/desktop/applications".into());

    let mut ids = HashSet::new();
    for dir in dirs {
        collect(&dir, &dir, &mut |id, _| {
            ids.insert(id);
        });
    }
    ids
}

/// Walks `dir` recursively, calling `f` with each `.desktop` file's ID and path.
/// The ID is the path relative to `base` with `/` replaced by `-`, per the spec.
fn collect(dir: &Path, base: &Path, f: &mut dyn FnMut(String, PathBuf)) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, base, f);
        } else if path.extension().is_some_and(|e| e == "desktop") {
            if let Ok(rel) = path.strip_prefix(base) {
                f(rel.to_string_lossy().replace('/', "-"), path);
            }
        }
    }
}

fn read_launcher(path: PathBuf, location: Location, is_override: bool) -> Option<Launcher> {
    let file = DesktopFile::load(&path).ok()?;
    if file.get("Type").as_deref() != Some("Application") {
        return None;
    }
    Some(Launcher {
        name: file.get("Name").unwrap_or_else(|| path.file_stem().unwrap().to_string_lossy().into()),
        icon: file.get("Icon").unwrap_or_default(),
        is_override: is_override || file.get_bool("Hidden"),
        path,
        location,
    })
}

pub fn scan(show_overrides: bool) -> Vec<Launcher> {
    let system = system_ids();
    let menu = menu_dir();
    let mut launchers = Vec::new();
    let mut menu_names = HashSet::new();

    collect(&menu, &menu, &mut |id, path| {
        menu_names.insert(path.file_name().unwrap().to_owned());
        if let Some(l) = read_launcher(path, Location::Menu, system.contains(&id)) {
            if show_overrides || !l.is_override {
                launchers.push(l);
            }
        }
    });

    // Desktop files sharing a name with a menu launcher are its copy; the editor manages those.
    if let Ok(entries) = fs::read_dir(desktop_dir()) {
        for path in entries.flatten().map(|e| e.path()) {
            let is_copy = path.file_name().is_some_and(|n| menu_names.contains(n));
            if path.extension().is_some_and(|e| e == "desktop") && !is_copy {
                launchers.extend(read_launcher(path, Location::Desktop, false));
            }
        }
    }

    launchers.sort_by_key(|l| l.name.to_lowercase());
    launchers
}
