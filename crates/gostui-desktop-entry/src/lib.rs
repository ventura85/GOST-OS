//! Parser for freedesktop `.desktop` entries.
//!
//! GostUI's Start Menu is a literal folder tree of shortcuts, so this parser sits
//! on the path of every launch. Two rules follow from that:
//!
//! - **Never panic on a malformed file.** These files come from every package on
//!   the system, and one bad line must not take the shell down.
//! - **Unknown keys are kept, not rejected.** The specification allows vendor
//!   extensions, and refusing them would break real applications.

#![forbid(unsafe_code)]

pub mod exec;

pub use exec::{expand, tokenize, ExecContext, ExecError};

use std::collections::BTreeMap;

/// The `Type=` of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    Application,
    Link,
    Directory,
    /// Anything else, kept rather than rejected.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// No `[Desktop Entry]` group was found.
    MissingDesktopEntryGroup,
    /// `Name=` is required by the specification.
    MissingName,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDesktopEntryGroup => write!(f, "no [Desktop Entry] group"),
            Self::MissingName => write!(f, "no Name= key"),
        }
    }
}

impl std::error::Error for ParseError {}

/// A parsed `.desktop` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    /// The application id: the file's basename without `.desktop`.
    pub id: String,
    pub entry_type: EntryType,
    /// Untranslated `Name=`.
    pub name: String,
    /// `Name[xx]=` variants, keyed by locale tag.
    pub localized_names: BTreeMap<String, String>,
    pub generic_name: Option<String>,
    pub comment: Option<String>,
    pub icon: Option<String>,
    pub exec: Option<String>,
    /// `TryExec=` — if set and not found on `PATH`, the entry should be hidden.
    pub try_exec: Option<String>,
    /// Working directory to launch in.
    pub path: Option<String>,
    pub terminal: bool,
    pub no_display: bool,
    /// `Hidden=true` means the entry is deleted as far as the user is concerned.
    pub hidden: bool,
    pub categories: Vec<String>,
    pub mime_types: Vec<String>,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    pub startup_notify: bool,
}

impl DesktopEntry {
    /// Parse the `[Desktop Entry]` group of a `.desktop` file.
    ///
    /// `id` is the application id, normally the basename without the extension.
    pub fn parse(id: impl Into<String>, text: &str) -> Result<Self, ParseError> {
        let mut in_group = false;
        let mut seen_group = false;
        let mut keys: BTreeMap<String, String> = BTreeMap::new();
        let mut localized_names = BTreeMap::new();

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(group) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                // Only the first [Desktop Entry] group counts; later groups are
                // actions, which we do not use yet.
                in_group = group == "Desktop Entry" && !seen_group;
                if in_group {
                    seen_group = true;
                }
                continue;
            }
            if !in_group {
                continue;
            }
            // A line without '=' is malformed. Skip it rather than failing the
            // whole file: one bad line must not hide an application.
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = unescape(value.trim());

            if let Some(locale) = key.strip_prefix("Name[").and_then(|k| k.strip_suffix(']')) {
                localized_names.insert(locale.to_string(), value);
                continue;
            }
            keys.insert(key.to_string(), value);
        }

        if !seen_group {
            return Err(ParseError::MissingDesktopEntryGroup);
        }
        let name = keys.get("Name").cloned().ok_or(ParseError::MissingName)?;

        Ok(Self {
            id: id.into(),
            entry_type: match keys.get("Type").map(String::as_str) {
                Some("Application") => EntryType::Application,
                Some("Link") => EntryType::Link,
                Some("Directory") => EntryType::Directory,
                _ => EntryType::Other,
            },
            name,
            localized_names,
            generic_name: keys.get("GenericName").cloned(),
            comment: keys.get("Comment").cloned(),
            icon: keys.get("Icon").cloned(),
            exec: keys.get("Exec").cloned(),
            try_exec: keys.get("TryExec").cloned(),
            path: keys.get("Path").cloned(),
            terminal: boolean(keys.get("Terminal")),
            no_display: boolean(keys.get("NoDisplay")),
            hidden: boolean(keys.get("Hidden")),
            categories: list(keys.get("Categories")),
            mime_types: list(keys.get("MimeType")),
            only_show_in: list(keys.get("OnlyShowIn")),
            not_show_in: list(keys.get("NotShowIn")),
            startup_notify: boolean(keys.get("StartupNotify")),
        })
    }

    /// Best display name for a locale tag such as `pl_PL.UTF-8`, `pl_PL` or `pl`.
    /// Falls back through the tag's prefixes and then to the untranslated name.
    pub fn name_for_locale(&self, locale: &str) -> &str {
        let base = locale.split('.').next().unwrap_or(locale);
        if let Some(n) = self.localized_names.get(base) {
            return n;
        }
        if let Some(lang) = base.split('_').next() {
            if let Some(n) = self.localized_names.get(lang) {
                return n;
            }
        }
        &self.name
    }

    /// Whether this entry belongs in a menu shown by `desktop` (e.g. `GostUI`).
    pub fn should_display(&self, desktop: &str) -> bool {
        if self.hidden || self.no_display {
            return false;
        }
        if self.not_show_in.iter().any(|d| d == desktop) {
            return false;
        }
        if !self.only_show_in.is_empty() && !self.only_show_in.iter().any(|d| d == desktop) {
            return false;
        }
        true
    }

    /// Build the argument vector to launch this entry with.
    ///
    /// Returns `None` when the entry has no `Exec=` — a `Link` or `Directory`
    /// entry, which is legal and simply not launchable this way.
    pub fn command(&self, ctx: &ExecContext<'_>) -> Option<Result<Vec<String>, ExecError>> {
        let exec = self.exec.as_ref()?;
        Some(tokenize(exec).and_then(|args| {
            let ctx = ExecContext {
                icon: ctx.icon.or(self.icon.as_deref()),
                name: ctx.name.or(Some(&self.name)),
                ..*ctx
            };
            expand(&args, &ctx)
        }))
    }
}

fn boolean(v: Option<&String>) -> bool {
    matches!(v.map(String::as_str), Some("true"))
}

fn list(v: Option<&String>) -> Vec<String> {
    v.map(|s| {
        s.split(';')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

/// Decode the escape sequences the specification defines for string values.
fn unescape(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
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

    const FIREFOX: &str = r#"
[Desktop Entry]
Version=1.0
Type=Application
Name=Firefox
Name[pl]=Przeglądarka Firefox
GenericName=Web Browser
Comment=Browse the Web
Exec=/usr/bin/firefox %u
Icon=firefox
Terminal=false
Categories=Network;WebBrowser;
MimeType=text/html;x-scheme-handler/https;
StartupNotify=true

[Desktop Action new-window]
Name=New Window
Exec=/usr/bin/firefox --new-window
"#;

    #[test]
    fn parses_a_realistic_entry() {
        let e = DesktopEntry::parse("firefox", FIREFOX).unwrap();
        assert_eq!(e.entry_type, EntryType::Application);
        assert_eq!(e.name, "Firefox");
        assert_eq!(e.icon.as_deref(), Some("firefox"));
        assert!(!e.terminal);
        assert_eq!(e.categories, ["Network", "WebBrowser"]);
        assert_eq!(e.mime_types.len(), 2);
        assert!(e.startup_notify);
    }

    #[test]
    fn action_groups_do_not_overwrite_the_main_group() {
        // [Desktop Action new-window] also has Name= and Exec=.
        let e = DesktopEntry::parse("firefox", FIREFOX).unwrap();
        assert_eq!(e.name, "Firefox");
        assert_eq!(e.exec.as_deref(), Some("/usr/bin/firefox %u"));
    }

    #[test]
    fn locale_falls_back_from_region_to_language_to_untranslated() {
        let e = DesktopEntry::parse("firefox", FIREFOX).unwrap();
        assert_eq!(e.name_for_locale("pl_PL.UTF-8"), "Przeglądarka Firefox");
        assert_eq!(e.name_for_locale("pl"), "Przeglądarka Firefox");
        assert_eq!(e.name_for_locale("de_DE"), "Firefox");
    }

    #[test]
    fn builds_a_command_line_with_a_url() {
        let e = DesktopEntry::parse("firefox", FIREFOX).unwrap();
        let uris = vec!["https://example.org".to_string()];
        let ctx = ExecContext {
            uris: &uris,
            ..Default::default()
        };
        assert_eq!(
            e.command(&ctx).unwrap().unwrap(),
            ["/usr/bin/firefox", "https://example.org"]
        );
    }

    #[test]
    fn visibility_honours_only_show_in_and_not_show_in() {
        let base = "[Desktop Entry]\nType=Application\nName=X\n";
        let plain = DesktopEntry::parse("x", base).unwrap();
        assert!(plain.should_display("GostUI"));

        let only = DesktopEntry::parse("x", &format!("{base}OnlyShowIn=GNOME;\n")).unwrap();
        assert!(!only.should_display("GostUI"));
        assert!(only.should_display("GNOME"));

        let not = DesktopEntry::parse("x", &format!("{base}NotShowIn=GostUI;\n")).unwrap();
        assert!(!not.should_display("GostUI"));

        let hidden = DesktopEntry::parse("x", &format!("{base}Hidden=true\n")).unwrap();
        assert!(!hidden.should_display("GostUI"));

        let nodisplay = DesktopEntry::parse("x", &format!("{base}NoDisplay=true\n")).unwrap();
        assert!(!nodisplay.should_display("GostUI"));
    }

    #[test]
    fn escape_sequences_are_decoded() {
        let e =
            DesktopEntry::parse("x", "[Desktop Entry]\nName=A\\sB\nComment=one\\ntwo\n").unwrap();
        assert_eq!(e.name, "A B");
        assert_eq!(e.comment.as_deref(), Some("one\ntwo"));
    }

    #[test]
    fn malformed_input_yields_errors_never_panics() {
        assert_eq!(
            DesktopEntry::parse("x", ""),
            Err(ParseError::MissingDesktopEntryGroup)
        );
        assert_eq!(
            DesktopEntry::parse("x", "[Desktop Entry]\nExec=/bin/true\n"),
            Err(ParseError::MissingName)
        );
        // A line with no '=' must not lose the rest of the file.
        let e = DesktopEntry::parse("x", "[Desktop Entry]\ngarbage line\nName=Y\n").unwrap();
        assert_eq!(e.name, "Y");
    }

    #[test]
    fn survives_junk_bytes_and_stray_brackets() {
        let junk = "[Desktop Entry]\nName=Z\n[[[\n=novalue\nKey=\n\u{fffd}\n";
        let e = DesktopEntry::parse("x", junk).unwrap();
        assert_eq!(e.name, "Z");
    }

    /// M0 acceptance criterion: parse every `.desktop` file present on this
    /// machine; none may panic.
    ///
    /// The test skips itself when there is nothing to read, so the suite passes
    /// in a build container and in a minimal Debian VM as well as on a desktop.
    /// It distinguishes "no files to test" (skip) from "files were found and none
    /// parsed" (failure) — the second means the parser is broken, and collapsing
    /// the two would let that go unnoticed on any host without applications
    /// installed.
    #[test]
    fn parses_every_desktop_file_on_this_system_without_panicking() {
        let dir = std::path::Path::new("/usr/share/applications");
        let Ok(entries) = std::fs::read_dir(dir) else {
            eprintln!("skipped: {} not present", dir.display());
            return;
        };

        let mut candidates = 0usize;
        let mut parsed = 0usize;
        let mut rejected = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            candidates += 1;
            // Not every file on a real system is valid UTF-8.
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let id = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            match DesktopEntry::parse(id, &text) {
                Ok(e) => {
                    parsed += 1;
                    // Field-code expansion must not panic either.
                    let _ = e.command(&ExecContext::default());
                }
                Err(_) => rejected += 1,
            }
        }

        if candidates == 0 {
            eprintln!("skipped: no .desktop files in {}", dir.display());
            return;
        }
        eprintln!("parsed {parsed} of {candidates} entries, rejected {rejected}");
        assert!(
            parsed > 0,
            "found {candidates} .desktop files in {} but parsed none",
            dir.display()
        );
    }
}
