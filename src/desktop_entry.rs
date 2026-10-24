//! Minimal reader/writer for freedesktop `.desktop` files.
//!
//! Every line of the original file is kept, so comments, translations,
//! `[Desktop Action ...]` groups and keys we don't know about all survive an edit.

use std::{fs, io, os::unix::fs::PermissionsExt, path::Path};

const MAIN_GROUP: &str = "Desktop Entry";

enum Line {
    Group(String),
    Entry { key: String, value: String },
    Raw(String),
}

pub struct DesktopFile {
    lines: Vec<Line>,
}

impl DesktopFile {
    pub fn new() -> Self {
        let mut file = Self { lines: vec![Line::Group(MAIN_GROUP.into())] };
        file.set("Type", "Application");
        file
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        Ok(Self::parse(&fs::read_to_string(path)?))
    }

    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    return Line::Group(trimmed[1..trimmed.len() - 1].to_string());
                }
                match line.split_once('=') {
                    Some((key, value)) if !trimmed.starts_with('#') => Line::Entry {
                        key: key.trim().to_string(),
                        value: value.trim_start().to_string(),
                    },
                    _ => Line::Raw(line.to_string()),
                }
            })
            .collect();
        Self { lines }
    }

    /// Line range of the `[Desktop Entry]` group: (header index, end exclusive).
    fn main_group(&self) -> Option<(usize, usize)> {
        let start = self
            .lines
            .iter()
            .position(|l| matches!(l, Line::Group(g) if g == MAIN_GROUP))?;
        let end = self.lines[start + 1..]
            .iter()
            .position(|l| matches!(l, Line::Group(_)))
            .map_or(self.lines.len(), |i| start + 1 + i);
        Some((start, end))
    }

    fn find(&self, key: &str) -> Option<usize> {
        let (start, end) = self.main_group()?;
        (start + 1..end).find(|&i| matches!(&self.lines[i], Line::Entry { key: k, .. } if k == key))
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match &self.lines[self.find(key)?] {
            Line::Entry { value, .. } => Some(unescape(value)),
            _ => None,
        }
    }

    pub fn get_bool(&self, key: &str) -> bool {
        self.get(key).is_some_and(|v| v.trim() == "true")
    }

    /// Sets a key in `[Desktop Entry]`; an empty value removes the key.
    pub fn set(&mut self, key: &str, value: &str) {
        if value.is_empty() {
            self.remove(key);
            return;
        }
        let line = Line::Entry { key: key.to_string(), value: escape(value) };
        if let Some(i) = self.find(key) {
            self.lines[i] = line;
            return;
        }
        let (start, end) = self.main_group().unwrap_or_else(|| {
            self.lines.insert(0, Line::Group(MAIN_GROUP.into()));
            (0, 1)
        });
        // Insert after the group's last entry so trailing blank lines stay put.
        let at = (start + 1..end)
            .rev()
            .find(|&i| matches!(self.lines[i], Line::Entry { .. }))
            .map_or(start + 1, |i| i + 1);
        self.lines.insert(at, line);
    }

    /// Writes `true`, or removes the key, which readers treat as `false`.
    pub fn set_flag(&mut self, key: &str, on: bool) {
        self.set(key, if on { "true" } else { "" });
    }

    pub fn remove(&mut self, key: &str) {
        if let Some(i) = self.find(key) {
            self.lines.remove(i);
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Group(g) => out.push_str(&format!("[{g}]")),
                Line::Entry { key, value } => out.push_str(&format!("{key}={value}")),
                Line::Raw(raw) => out.push_str(raw),
            }
            out.push('\n');
        }
        out
    }

    /// Writes the file and marks it executable, which Nemo requires for desktop launchers.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        fs::write(path, self.to_text())?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
    }
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for (i, c) in value.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            ' ' if i == 0 => out.push_str("\\s"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            // Unknown escapes (e.g. `\;` in lists) are kept verbatim.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# comment\n[Desktop Entry]\nType=Application\nName=Foo\nName[de]=Fu\nExec=foo %U\n\n[Desktop Action new]\nName=New\nExec=foo --new\n";

    #[test]
    fn round_trips_untouched_file() {
        assert_eq!(DesktopFile::parse(SAMPLE).to_text(), SAMPLE);
    }

    #[test]
    fn edits_only_main_group() {
        let mut f = DesktopFile::parse(SAMPLE);
        assert_eq!(f.get("Exec").as_deref(), Some("foo %U"));
        f.set("Exec", "bar");
        f.set("Icon", "bar-icon");
        let text = f.to_text();
        assert!(text.contains("Exec=bar\nIcon=bar-icon\n\n[Desktop Action new]"));
        assert!(text.contains("Exec=foo --new"));
        f.set("Icon", "");
        assert_eq!(f.get("Icon"), None);
    }

    #[test]
    fn escapes_round_trip() {
        let mut f = DesktopFile::new();
        f.set("Comment", " line1\nback\\slash");
        assert!(f.to_text().contains("Comment=\\sline1\\nback\\\\slash"));
        assert_eq!(f.get("Comment").as_deref(), Some(" line1\nback\\slash"));
    }
}
