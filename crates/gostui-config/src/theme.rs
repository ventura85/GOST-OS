//! Reading and writing `theme.toml` (D-032).
//!
//! The theme's shape and every rule about it live in `gostui-core`. This module
//! does one job: turn a file into a [`Theme`] and back. That split is why
//! `gostui-core` has no `serde` — the file format is a detail of storage, and a
//! change to it must not be able to reach the logic (D-016).
//!
//! **Every field is optional.** A user who wants a different accent colour
//! writes three lines, not a hundred; anything absent keeps the built-in value.
//! Personalisation that demands a complete file is personalisation nobody uses.
//!
//! **Nothing here can stop the shell from starting.** A missing file is a first
//! run, a corrupt file falls back to the built-in theme, and a single unreadable
//! colour costs that one colour rather than the whole theme. Every fallback is
//! reported so it can be logged: a theme that silently "did not apply" is the
//! worst of the available outcomes.

use crate::{atomic, ConfigError, SCHEMA_VERSION};
use gostui_core::theme::{Fonts, Metrics, Palette, Pointing, Report, Rgba, Theme};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// `$XDG_CONFIG_HOME/gostui/theme.toml`, alongside `config.toml`.
pub fn default_path() -> Option<PathBuf> {
    Some(crate::default_path()?.with_file_name("theme.toml"))
}

/// A value in the file that could not be used, and what was used instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProblem {
    /// Dotted path as it appears in the file, e.g. `palette.card`.
    pub field: String,
    /// What the file said.
    pub value: String,
}

impl std::fmt::Display for FieldProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} = {:?} is not a colour (expected #rrggbb or #rrggbbaa); kept the built-in value",
            self.field, self.value
        )
    }
}

/// Everything worth logging about one attempt to load a theme.
#[derive(Debug, Default)]
pub struct ThemeReport {
    /// The file could not be read or parsed at all; the built-in theme is in use.
    pub file: Option<ConfigError>,
    /// Colours that did not parse. Each costs one role, not the theme.
    pub colours: Vec<FieldProblem>,
    /// What `gostui-core` corrected, and which surfaces collapse at 16 bits.
    pub core: Report,
}

impl ThemeReport {
    /// True when the theme was taken exactly as written.
    pub fn is_clean(&self) -> bool {
        self.file.is_none() && self.colours.is_empty() && self.core.is_clean()
    }

    /// One line per problem, ready for the log. Empty when clean.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(e) = &self.file {
            out.push(format!("{e}; using the built-in theme"));
        }
        out.extend(self.colours.iter().map(|p| p.to_string()));
        out.extend(
            self.core
                .adjustments
                .iter()
                .map(|a| format!("{} = {} raised to {} ({})", a.field, a.from, a.to, a.reason)),
        );
        out.extend(
            self.core
                .low_contrast
                .iter()
                .map(|(a, b)| format!("{a} and {b} are indistinguishable on a 16-bit framebuffer")),
        );
        out
    }
}

/// The file as written on disk: every field optional, colours as hex strings.
///
/// `deny_unknown_fields` is deliberate even though it makes one typo cost the
/// whole file. The alternative — ignoring keys we do not recognise — produces a
/// theme that half-applies with nothing to explain why, and that is the failure
/// mode users cannot debug. A rejected file at least names the offending key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ThemeFile {
    pub version: Option<u32>,
    pub name: Option<String>,
    pub palette: PaletteFile,
    pub metrics: MetricsFile,
    pub fonts: FontsFile,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PaletteFile {
    pub desktop: Option<String>,
    pub bar: Option<String>,
    pub bar_edge: Option<String>,
    pub chip: Option<String>,
    pub card: Option<String>,
    pub card_active: Option<String>,
    pub accent: Option<String>,
    pub accent_alt: Option<String>,
    pub tile: Option<String>,
    pub text: Option<String>,
    pub text_dim: Option<String>,
    pub focus_ring: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MetricsFile {
    pub top_bar: Option<i32>,
    pub bottom_bar: Option<i32>,
    pub card_header: Option<i32>,
    pub card_width: Option<i32>,
    pub card_gap: Option<i32>,
    pub card_pad: Option<i32>,
    pub tile_unit: Option<i32>,
    pub tile_gap: Option<i32>,
    pub inner_gap: Option<i32>,
    pub outer_gap: Option<i32>,
    pub focus_width: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FontsFile {
    pub ui: Option<String>,
    pub mono: Option<String>,
    pub size_bar: Option<i32>,
    pub size_tile: Option<i32>,
    pub size_tile_value: Option<i32>,
}

/// Apply one optional hex colour over a built-in value, recording a failure.
fn colour(
    field: &str,
    written: &Option<String>,
    builtin: Rgba,
    problems: &mut Vec<FieldProblem>,
) -> Rgba {
    let Some(text) = written else { return builtin };
    match Rgba::parse_hex(text) {
        Some(c) => c,
        None => {
            problems.push(FieldProblem {
                field: field.to_string(),
                value: text.clone(),
            });
            builtin
        }
    }
}

impl ThemeFile {
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let file: Self = toml::from_str(text).map_err(|e| ConfigError::Parse(e.to_string()))?;
        match file.version {
            Some(v) if v > SCHEMA_VERSION => Err(ConfigError::UnsupportedVersion {
                found: v,
                supported: SCHEMA_VERSION,
            }),
            _ => Ok(file),
        }
    }

    /// Merge over the built-in theme. Absent fields keep the built-in value.
    pub fn into_theme(self) -> (Theme, Vec<FieldProblem>) {
        let base = Theme::builtin();
        let bp = base.palette;
        let mut problems = Vec::new();
        let p = &self.palette;

        let palette = Palette {
            desktop: colour("palette.desktop", &p.desktop, bp.desktop, &mut problems),
            bar: colour("palette.bar", &p.bar, bp.bar, &mut problems),
            bar_edge: colour("palette.bar_edge", &p.bar_edge, bp.bar_edge, &mut problems),
            chip: colour("palette.chip", &p.chip, bp.chip, &mut problems),
            card: colour("palette.card", &p.card, bp.card, &mut problems),
            card_active: colour(
                "palette.card_active",
                &p.card_active,
                bp.card_active,
                &mut problems,
            ),
            accent: colour("palette.accent", &p.accent, bp.accent, &mut problems),
            accent_alt: colour(
                "palette.accent_alt",
                &p.accent_alt,
                bp.accent_alt,
                &mut problems,
            ),
            tile: colour("palette.tile", &p.tile, bp.tile, &mut problems),
            text: colour("palette.text", &p.text, bp.text, &mut problems),
            text_dim: colour("palette.text_dim", &p.text_dim, bp.text_dim, &mut problems),
            focus_ring: colour(
                "palette.focus_ring",
                &p.focus_ring,
                bp.focus_ring,
                &mut problems,
            ),
        };

        let m = &self.metrics;
        let bm = base.metrics;
        let metrics = Metrics {
            top_bar: m.top_bar.unwrap_or(bm.top_bar),
            bottom_bar: m.bottom_bar.unwrap_or(bm.bottom_bar),
            card_header: m.card_header.unwrap_or(bm.card_header),
            card_width: m.card_width.unwrap_or(bm.card_width),
            card_gap: m.card_gap.unwrap_or(bm.card_gap),
            card_pad: m.card_pad.unwrap_or(bm.card_pad),
            tile_unit: m.tile_unit.unwrap_or(bm.tile_unit),
            tile_gap: m.tile_gap.unwrap_or(bm.tile_gap),
            inner_gap: m.inner_gap.unwrap_or(bm.inner_gap),
            outer_gap: m.outer_gap.unwrap_or(bm.outer_gap),
            focus_width: m.focus_width.unwrap_or(bm.focus_width),
        };

        let f = self.fonts;
        let bf = base.fonts;
        let fonts = Fonts {
            ui: f.ui.unwrap_or(bf.ui),
            mono: f.mono.unwrap_or(bf.mono),
            size_bar: f.size_bar.unwrap_or(bf.size_bar),
            size_tile: f.size_tile.unwrap_or(bf.size_tile),
            size_tile_value: f.size_tile_value.unwrap_or(bf.size_tile_value),
        };

        let theme = Theme {
            name: self.name.unwrap_or(base.name),
            palette,
            metrics,
            fonts,
        };
        (theme, problems)
    }

    /// A complete file describing `theme`.
    ///
    /// Complete rather than minimal on purpose: this is what the user opens to
    /// find out what can be changed, so every role has to be visible even when
    /// it holds the default.
    pub fn from_theme(theme: &Theme) -> Self {
        let p = &theme.palette;
        let m = &theme.metrics;
        let f = &theme.fonts;
        Self {
            version: Some(SCHEMA_VERSION),
            name: Some(theme.name.clone()),
            palette: PaletteFile {
                desktop: Some(p.desktop.to_hex()),
                bar: Some(p.bar.to_hex()),
                bar_edge: Some(p.bar_edge.to_hex()),
                chip: Some(p.chip.to_hex()),
                card: Some(p.card.to_hex()),
                card_active: Some(p.card_active.to_hex()),
                accent: Some(p.accent.to_hex()),
                accent_alt: Some(p.accent_alt.to_hex()),
                tile: Some(p.tile.to_hex()),
                text: Some(p.text.to_hex()),
                text_dim: Some(p.text_dim.to_hex()),
                focus_ring: Some(p.focus_ring.to_hex()),
            },
            metrics: MetricsFile {
                top_bar: Some(m.top_bar),
                bottom_bar: Some(m.bottom_bar),
                card_header: Some(m.card_header),
                card_width: Some(m.card_width),
                card_gap: Some(m.card_gap),
                card_pad: Some(m.card_pad),
                tile_unit: Some(m.tile_unit),
                tile_gap: Some(m.tile_gap),
                inner_gap: Some(m.inner_gap),
                outer_gap: Some(m.outer_gap),
                focus_width: Some(m.focus_width),
            },
            fonts: FontsFile {
                ui: Some(f.ui.clone()),
                mono: Some(f.mono.clone()),
                size_bar: Some(f.size_bar),
                size_tile: Some(f.size_tile),
                size_tile_value: Some(f.size_tile_value),
            },
        }
    }

    pub fn to_toml(&self) -> String {
        // Serialising our own type cannot fail; a panic here would be a bug in
        // this crate, not bad user input.
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

/// Load a theme, correcting and reporting rather than failing.
///
/// Always returns a usable theme. `pointing` decides how small a target the
/// theme is allowed to make: a touch-only session holds the 48-unit floor, a
/// docked one with a mouse does not (D-030, D-032).
pub fn load(path: &Path, pointing: Pointing) -> (Theme, ThemeReport) {
    let mut report = ThemeReport::default();

    let file = match std::fs::read_to_string(path) {
        Ok(text) => match ThemeFile::parse(&text) {
            Ok(f) => f,
            Err(e) => {
                report.file = Some(e);
                ThemeFile::default()
            }
        },
        // A missing file is a first run, not a fault.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ThemeFile::default(),
        Err(e) => {
            report.file = Some(ConfigError::Io(e));
            ThemeFile::default()
        }
    };

    let (theme, colours) = file.into_theme();
    report.colours = colours;
    let (theme, core) = theme.sanitised(pointing);
    report.core = core;
    (theme, report)
}

/// Write a complete theme file, atomically (a crash mid-save must not leave a
/// file the shell cannot read).
pub fn save(theme: &Theme, path: &Path) -> Result<(), ConfigError> {
    atomic::write(path, &ThemeFile::from_theme(theme).to_toml())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gostui_core::MIN_TOUCH_TARGET;

    #[test]
    fn an_empty_file_is_the_builtin_theme() {
        let (theme, problems) = ThemeFile::default().into_theme();
        assert_eq!(theme, Theme::builtin());
        assert!(problems.is_empty());
    }

    #[test]
    fn three_lines_change_one_colour_and_nothing_else() {
        // The property that decides whether anyone actually themes this.
        let (theme, problems) = ThemeFile::parse("[palette]\naccent = \"#ff0000\"\n")
            .unwrap()
            .into_theme();
        assert!(problems.is_empty());
        assert_eq!(theme.palette.accent, Rgba::rgb(0xff, 0, 0));
        let builtin = Theme::builtin();
        assert_eq!(theme.palette.card, builtin.palette.card);
        assert_eq!(theme.metrics, builtin.metrics);
        assert_eq!(theme.fonts, builtin.fonts);
    }

    #[test]
    fn one_bad_colour_costs_one_colour_not_the_theme() {
        let text = "[palette]\naccent = \"zielony\"\ncard = \"#101010\"\n";
        let (theme, problems) = ThemeFile::parse(text).unwrap().into_theme();
        assert_eq!(theme.palette.accent, Theme::builtin().palette.accent);
        assert_eq!(theme.palette.card, Rgba::rgb(0x10, 0x10, 0x10));
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].field, "palette.accent");
        // The message has to name the field and the value, or it is useless in
        // a log.
        let msg = problems[0].to_string();
        assert!(
            msg.contains("palette.accent") && msg.contains("zielony"),
            "{msg}"
        );
    }

    #[test]
    fn a_corrupt_file_yields_the_builtin_theme_and_an_explanation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.toml");
        std::fs::write(&path, "nie jest to toml {{{").unwrap();
        let (theme, report) = load(&path, Pointing::Touch);
        assert_eq!(theme, Theme::builtin());
        assert!(matches!(report.file, Some(ConfigError::Parse(_))));
        assert!(!report.lines().is_empty());
    }

    #[test]
    fn a_missing_file_is_a_first_run_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (theme, report) = load(&dir.path().join("nope.toml"), Pointing::Touch);
        assert_eq!(theme, Theme::builtin());
        assert!(report.is_clean(), "{:?}", report.lines());
    }

    #[test]
    fn an_unknown_key_is_reported_rather_than_ignored() {
        // Ignoring it would give a theme that half-applies with nothing to
        // explain why.
        let err = ThemeFile::parse("[palette]\nakcent = \"#ff0000\"\n").unwrap_err();
        assert!(matches!(err, ConfigError::Parse(_)));
        assert!(format!("{err}").contains("akcent"));
    }

    #[test]
    fn a_future_schema_version_is_reported_not_guessed_at() {
        let text = format!("version = {}\n", SCHEMA_VERSION + 1);
        assert!(matches!(
            ThemeFile::parse(&text),
            Err(ConfigError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn a_theme_that_shrinks_touch_targets_is_corrected_on_a_touch_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.toml");
        // 32 is above the pointer floor and below the touch one — the only band
        // where the two sessions disagree, which is the whole point of D-030.
        std::fs::write(&path, "[metrics]\ntop_bar = 32\n").unwrap();

        let (touch, report) = load(&path, Pointing::Touch);
        assert_eq!(touch.metrics.top_bar, MIN_TOUCH_TARGET);
        assert_eq!(report.core.adjustments.len(), 1);

        // The same file on a docked session keeps what it asked for (D-030).
        let (pointer, report) = load(&path, Pointing::Pointer);
        assert_eq!(pointer.metrics.top_bar, 32);
        assert!(report.core.adjustments.is_empty());
    }

    #[test]
    fn a_low_contrast_theme_loads_but_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.toml");
        // Two navies one step apart: fine at 24 bits, one colour at 16.
        std::fs::write(
            &path,
            "[palette]\ncard = \"#1b2a44\"\ncard_active = \"#1c2b45\"\n",
        )
        .unwrap();
        let (theme, report) = load(&path, Pointing::Touch);
        // Loaded as written: the user is not overruled on colour.
        assert_eq!(theme.palette.card_active, Rgba::rgb(0x1c, 0x2b, 0x45));
        assert_eq!(report.core.low_contrast, vec![("card", "card_active")]);
        assert!(!report.is_clean());
    }

    #[test]
    fn a_saved_theme_round_trips_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.toml");
        let mut theme = Theme::builtin();
        theme.palette.accent = Rgba::rgb(0xff, 0x00, 0x88);
        theme.metrics.tile_unit = 120;
        theme.fonts.ui = "Inter".to_string();
        save(&theme, &path).unwrap();

        let (loaded, report) = load(&path, Pointing::Touch);
        assert_eq!(loaded, theme);
        assert!(report.is_clean(), "{:?}", report.lines());
    }

    #[test]
    fn a_written_file_shows_every_role_the_user_may_change() {
        // The file is the documentation of what is themeable, so nothing may be
        // omitted just because it holds the default.
        let toml = ThemeFile::from_theme(&Theme::builtin()).to_toml();
        for role in [
            "desktop",
            "bar",
            "bar_edge",
            "chip",
            "card",
            "card_active",
            "accent",
            "accent_alt",
            "tile",
            "text",
            "text_dim",
            "focus_ring",
        ] {
            assert!(toml.contains(role), "{role} missing from the written file");
        }
        assert!(toml.contains("tile_unit") && toml.contains("size_bar"));
    }

    #[test]
    fn alpha_survives_the_round_trip() {
        let mut theme = Theme::builtin();
        theme.palette.tile = Rgba(0x2d, 0x44, 0x60, 0x80);
        let toml = ThemeFile::from_theme(&theme).to_toml();
        let (back, problems) = ThemeFile::parse(&toml).unwrap().into_theme();
        assert!(problems.is_empty());
        assert_eq!(back.palette.tile, Rgba(0x2d, 0x44, 0x60, 0x80));
    }
}
