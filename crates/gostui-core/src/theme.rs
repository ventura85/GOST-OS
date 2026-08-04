//! Appearance as data: colours, sizes and font families (D-032).
//!
//! Every number the interface is drawn with lives here rather than as a `const`
//! in the rasteriser, because the user is meant to change all of it. That is the
//! whole decision; the rest of this module is the two safeguards without which
//! "maximum personalisation" turns into a system the user can break.
//!
//! **A theme cannot make the shell untouchable.** [`Metrics::sanitised`] raises
//! anything the finger must hit back to [`MIN_TOUCH_TARGET`] on a touch session.
//! It reports what it changed instead of rejecting the theme, so a bad file
//! costs the user a log line, not a session (see the resilience rule in D-027).
//!
//! **A theme cannot make the shell invisible.** [`Palette::low_contrast_pairs`]
//! catches surfaces that stop being distinguishable once the picture is
//! quantised to RGB565 — the format a machine with no GPU driver may well be
//! scanning out (D-001, D-027).
//!
//! Colours and sizes are here in `gostui-core` and not in the rasteriser for the
//! same reason layout is (D-016): choosing a bar height is arithmetic with a
//! testable answer, and `cargo test` must be able to reach it.

use crate::layout::Gaps;
use crate::shell::{BarHeights, MIN_TOUCH_TARGET};

/// An sRGB colour with straight alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b, 255)
    }

    /// Parse `#rrggbb` or `#rrggbbaa`, with or without the leading `#`.
    ///
    /// Returns `None` rather than a default colour: silently accepting a typo
    /// would leave the user staring at a theme that "did not apply" with nothing
    /// to explain why. The caller reports the field and falls back (D-032).
    pub fn parse_hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#').unwrap_or(s);
        if !s.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            6 => Some(Self(byte(0)?, byte(2)?, byte(4)?, 255)),
            8 => Some(Self(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// Format as `#rrggbb`, or `#rrggbbaa` when not fully opaque.
    pub fn to_hex(self) -> String {
        if self.3 == 255 {
            format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.0, self.1, self.2, self.3)
        }
    }

    /// This colour as it survives a 16-bit RGB565 framebuffer: 5 bits of red,
    /// 6 of green, 5 of blue.
    ///
    /// Not a curiosity. A machine with no GPU driver scans out of whatever the
    /// firmware handed the kernel, and on older hardware that is RGB565. Two
    /// navies three percent apart are two different colours at 24 bits and one
    /// colour at 16.
    pub const fn quantise_565(self) -> (u8, u8, u8) {
        // +127 rounds to nearest without floating point. Written out rather
        // than through a closure because a `const fn` cannot call one.
        (
            (((self.0 as u32 * 31) + 127) / 255) as u8,
            (((self.1 as u32 * 63) + 127) / 255) as u8,
            (((self.2 as u32 * 31) + 127) / 255) as u8,
        )
    }
}

/// The colour roles the shell draws with.
///
/// Roles, not colour names: the user replaces "the surface a card sits on", not
/// "dark navy". A field here is a promise that every place drawing that kind of
/// thing reads this one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Behind everything; visible where nothing else is drawn.
    pub desktop: Rgba,
    /// The two bars.
    pub bar: Rgba,
    /// The rule separating a bar from the application area.
    pub bar_edge: Rgba,
    /// Small raised elements inside a bar: clock, status, window buttons.
    pub chip: Rgba,
    /// A card of the slider.
    pub card: Rgba,
    /// The card that has focus.
    pub card_active: Rgba,
    /// Brand colour, used for the anchor of the bar and the focus ring.
    pub accent: Rgba,
    /// Second brand colour, used where the first would collide.
    pub accent_alt: Rgba,
    /// A tile on a card.
    pub tile: Rgba,
    /// Body text. Unused until the text stack lands (D-005), defined now so the
    /// theme file does not change shape when it does.
    pub text: Rgba,
    /// Secondary text: captions, values that are not the point of the tile.
    pub text_dim: Rgba,
    /// Ring around the focused element. Separate from `accent` because focus
    /// must stay visible in a theme that makes the accent quiet.
    pub focus_ring: Rgba,
}

impl Default for Palette {
    /// Taken from the GOST OS logo: navy, cyan and lime.
    fn default() -> Self {
        Self {
            desktop: Rgba::rgb(0x0b, 0x12, 0x20),
            bar: Rgba::rgb(0x14, 0x20, 0x33),
            bar_edge: Rgba::rgb(0x1e, 0x3a, 0x8f),
            chip: Rgba::rgb(0x24, 0x37, 0x4f),
            card: Rgba::rgb(0x1b, 0x2a, 0x44),
            card_active: Rgba::rgb(0x22, 0x35, 0x55),
            accent: Rgba::rgb(0x22, 0xc8, 0xe8),
            accent_alt: Rgba::rgb(0xb5, 0xd3, 0x34),
            tile: Rgba::rgb(0x2d, 0x44, 0x60),
            text: Rgba::rgb(0xe6, 0xed, 0xf5),
            text_dim: Rgba::rgb(0x8d, 0xa2, 0xba),
            focus_ring: Rgba::rgb(0x22, 0xc8, 0xe8),
        }
    }
}

/// Two roles that must not collapse into one another, and the name of the pair.
type Neighbours = (&'static str, &'static str);

impl Palette {
    /// Pairs of roles that stop being distinguishable on a 16-bit framebuffer.
    ///
    /// Only pairs that actually touch on screen are checked — `desktop` against
    /// `text` proves nothing, because they never share an edge. The threshold is
    /// a judgement call, stated so it can be argued with: a pair passes when its
    /// three quantised channels differ by **2 steps in total**. One step in a
    /// single channel is a difference the eye loses against a dithered gradient
    /// or a cheap panel.
    pub fn low_contrast_pairs(&self) -> Vec<Neighbours> {
        let pairs: [(Neighbours, Rgba, Rgba); 7] = [
            (("desktop", "bar"), self.desktop, self.bar),
            (("desktop", "card"), self.desktop, self.card),
            (("bar", "chip"), self.bar, self.chip),
            (("card", "card_active"), self.card, self.card_active),
            (("card", "tile"), self.card, self.tile),
            (("card_active", "tile"), self.card_active, self.tile),
            (("bar", "bar_edge"), self.bar, self.bar_edge),
        ];
        pairs
            .into_iter()
            .filter(|&(_, a, b)| distance_565(a, b) < 2)
            .map(|(names, _, _)| names)
            .collect()
    }

    /// Same check for the text roles, which only matter once text exists (D-005).
    pub fn low_contrast_text_pairs(&self) -> Vec<Neighbours> {
        let pairs: [(Neighbours, Rgba, Rgba); 4] = [
            (("text", "bar"), self.text, self.bar),
            (("text", "card"), self.text, self.card),
            (("text_dim", "card"), self.text_dim, self.card),
            (("text_dim", "tile"), self.text_dim, self.tile),
        ];
        pairs
            .into_iter()
            .filter(|&(_, a, b)| distance_565(a, b) < 2)
            .map(|(names, _, _)| names)
            .collect()
    }
}

/// Total quantised channel difference between two colours at 16 bits.
fn distance_565(a: Rgba, b: Rgba) -> u32 {
    let (ar, ag, ab) = a.quantise_565();
    let (br, bg, bb) = b.quantise_565();
    let d = |x: u8, y: u8| x.abs_diff(y) as u32;
    d(ar, br) + d(ag, bg) + d(ab, bb)
}

/// What the user points with. Decides how far a theme may shrink a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pointing {
    /// Finger only. Touch targets are held at [`MIN_TOUCH_TARGET`].
    #[default]
    Touch,
    /// A mouse or trackpad is present — a docked phone, or any desktop.
    ///
    /// The touch floor does not apply here, and that is not a loosening for its
    /// own sake: it is what makes landscape on a phone affordable (D-030). Two
    /// 48-unit bars are 27% of a 360-unit-high screen and 9% of a monitor.
    Pointer,
}

impl Pointing {
    /// The smallest a hittable element may be under this input.
    ///
    /// The pointer floor is not zero: an element of one logical unit is a bug
    /// whoever is pointing, and a theme should not be able to produce it.
    pub const fn floor(self) -> i32 {
        match self {
            Self::Touch => MIN_TOUCH_TARGET,
            Self::Pointer => 16,
        }
    }
}

/// Every size the interface is drawn with, in logical units (D-011).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    pub top_bar: i32,
    pub bottom_bar: i32,
    /// Height of a card's header, where its name goes (D-046).
    pub card_header: i32,
    /// Width of one card column. How many are on screen follows from this and
    /// the width of the output — the count is never configured directly, which
    /// is what lets one card look the same on a monitor and on a phone (D-046).
    pub card_width: i32,
    /// Gap between card columns.
    pub card_gap: i32,
    /// Margin between a card's edge and its contents.
    pub card_pad: i32,
    /// One cell of the tile grid. A 2×2 tile is two of these plus one gap.
    pub tile_unit: i32,
    pub tile_gap: i32,
    /// Gap between tiled windows.
    pub inner_gap: i32,
    /// Gap between the tiled area and the screen edge.
    pub outer_gap: i32,
    /// Thickness of the focus ring.
    pub focus_width: i32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            top_bar: MIN_TOUCH_TARGET,
            bottom_bar: MIN_TOUCH_TARGET,
            card_header: MIN_TOUCH_TARGET,
            // 260 puts seven cards on a 1920 monitor and two-and-a-sliver on a
            // phone held sideways, and leaves room for two tile columns inside.
            card_width: 260,
            card_gap: 12,
            card_pad: 12,
            tile_unit: 96,
            tile_gap: 12,
            inner_gap: 4,
            outer_gap: 0,
            focus_width: 2,
        }
    }
}

impl Metrics {
    /// Bar heights in the shape [`crate::shell::zones`] wants.
    pub const fn bar_heights(&self) -> BarHeights {
        BarHeights {
            top: self.top_bar,
            bottom: self.bottom_bar,
        }
    }

    /// Tiling gaps in the shape [`crate::layout::tile`] wants.
    pub const fn gaps(&self) -> Gaps {
        Gaps {
            outer: self.outer_gap,
            inner: self.inner_gap,
        }
    }

    /// Clamp to values that cannot produce an unusable interface, reporting
    /// every change.
    ///
    /// Never fails and never rejects: a theme file is user input, and the cost
    /// of refusing it is a session that will not start. The caller logs the
    /// adjustments so a theme that "did not take" can be explained.
    pub fn sanitised(self, pointing: Pointing) -> (Self, Vec<Adjustment>) {
        let mut out = self;
        let mut log = Vec::new();
        let floor = pointing.floor();

        let mut clamp = |field: &'static str, value: &mut i32, min: i32, reason: &'static str| {
            if *value < min {
                log.push(Adjustment {
                    field,
                    from: *value,
                    to: min,
                    reason,
                });
                *value = min;
            }
        };

        clamp("top_bar", &mut out.top_bar, floor, TOUCHABLE);
        clamp("bottom_bar", &mut out.bottom_bar, floor, TOUCHABLE);
        clamp("card_header", &mut out.card_header, floor, TOUCHABLE);
        clamp("card_width", &mut out.card_width, floor, TOUCHABLE);
        clamp("tile_unit", &mut out.tile_unit, floor, TOUCHABLE);
        // Gaps may legitimately be zero; they may not be negative, which would
        // make a tile larger than the area it was cut from.
        clamp("card_gap", &mut out.card_gap, 0, NON_NEGATIVE);
        clamp("card_pad", &mut out.card_pad, 0, NON_NEGATIVE);
        clamp("tile_gap", &mut out.tile_gap, 0, NON_NEGATIVE);
        clamp("inner_gap", &mut out.inner_gap, 0, NON_NEGATIVE);
        clamp("outer_gap", &mut out.outer_gap, 0, NON_NEGATIVE);
        // A focus ring of zero units is a focus ring nobody can see, and focus
        // has to be readable without hovering (D-020).
        clamp("focus_width", &mut out.focus_width, 1, VISIBLE);

        (out, log)
    }
}

const TOUCHABLE: &str = "below the smallest target this input can hit";
const NON_NEGATIVE: &str = "a negative size is not a size";
const VISIBLE: &str = "would not be visible at all";

/// One value a theme asked for and did not get.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adjustment {
    pub field: &'static str,
    pub from: i32,
    pub to: i32,
    pub reason: &'static str,
}

/// Font families and sizes.
///
/// Families are named the way fontconfig names them — `"Inter"`, `"DejaVu
/// Sans"` — and never as paths. A path is wrong on the next machine, and
/// `cosmic-text` (D-005) resolves families through fontconfig anyway. An empty
/// family means "whatever the system calls sans-serif", which is the only
/// sensible default on a system we do not control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fonts {
    pub ui: String,
    pub mono: String,
    /// Size of text in the bars.
    pub size_bar: i32,
    /// Size of a tile caption.
    pub size_tile: i32,
    /// Size of the value on a live tile — the number the tile exists to show
    /// (D-033), so it is deliberately not the same size as the caption.
    pub size_tile_value: i32,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            ui: String::new(),
            mono: String::new(),
            size_bar: 14,
            size_tile: 12,
            size_tile_value: 20,
        }
    }
}

impl Fonts {
    /// Smallest font size that is still text rather than a smudge.
    pub const MIN_SIZE: i32 = 6;

    /// How tall one line of text at `size` needs its box to be.
    ///
    /// Layout has to reserve the room before anything is shaped — the caption
    /// strip of a tile is decided in core, where no font exists (D-016) — so the
    /// factor lives here rather than being asked of the rasteriser. It matches
    /// the line height the text stack uses, rounded up: a box shorter than the
    /// line would centre the glyphs and let them stick out top and bottom.
    pub const fn line_height(size: i32) -> i32 {
        if size <= 0 {
            return 0;
        }
        (size * 3 + 1) / 2
    }

    pub fn sanitised(self) -> (Self, Vec<Adjustment>) {
        let mut out = self;
        let mut log = Vec::new();
        let mut clamp = |field: &'static str, value: &mut i32| {
            if *value < Self::MIN_SIZE {
                log.push(Adjustment {
                    field,
                    from: *value,
                    to: Self::MIN_SIZE,
                    reason: "too small to read",
                });
                *value = Self::MIN_SIZE;
            }
        };
        clamp("size_bar", &mut out.size_bar);
        clamp("size_tile", &mut out.size_tile);
        clamp("size_tile_value", &mut out.size_tile_value);
        (out, log)
    }
}

/// A complete appearance: what to draw with, how big, in what typeface.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Theme {
    /// Shown in the settings panel; has no effect on drawing.
    pub name: String,
    pub palette: Palette,
    pub metrics: Metrics,
    pub fonts: Fonts,
}

impl Theme {
    /// The built-in theme. This is the one a broken file falls back to, so it
    /// must always be valid — asserted in the tests below, not assumed.
    pub fn builtin() -> Self {
        Self {
            name: "GOST".to_string(),
            ..Self::default()
        }
    }

    /// Everything wrong with this theme, with the corrections already applied.
    ///
    /// One entry point so a caller cannot check the sizes and forget the
    /// colours. Colour problems are reported but not corrected: there is no
    /// defensible automatic answer to "these two navies are too close", and
    /// picking one would overrule the user on the very thing they came to
    /// change.
    pub fn sanitised(self, pointing: Pointing) -> (Self, Report) {
        let (metrics, metric_adjustments) = self.metrics.sanitised(pointing);
        let (fonts, font_adjustments) = self.fonts.sanitised();
        let report = Report {
            adjustments: metric_adjustments
                .into_iter()
                .chain(font_adjustments)
                .collect(),
            low_contrast: self.palette.low_contrast_pairs(),
        };
        (
            Self {
                name: self.name,
                palette: self.palette,
                metrics,
                fonts,
            },
            report,
        )
    }
}

/// What [`Theme::sanitised`] had to say about a theme.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Values that were corrected.
    pub adjustments: Vec<Adjustment>,
    /// Role pairs that collapse on a 16-bit framebuffer. A warning, not an
    /// error: on a 24-bit output they are fine, and most outputs are 24-bit.
    pub low_contrast: Vec<Neighbours>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.adjustments.is_empty() && self.low_contrast.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let c = Rgba::parse_hex("#22c8e8").unwrap();
        assert_eq!(c, Rgba::rgb(0x22, 0xc8, 0xe8));
        assert_eq!(c.to_hex(), "#22c8e8");
        let a = Rgba::parse_hex("22c8e880").unwrap();
        assert_eq!(a, Rgba(0x22, 0xc8, 0xe8, 0x80));
        assert_eq!(a.to_hex(), "#22c8e880");
    }

    #[test]
    fn a_malformed_colour_is_rejected_not_guessed() {
        for bad in [
            "", "#", "#12345", "#gggggg", "#1234567", "zielony", "#12345g",
        ] {
            assert!(Rgba::parse_hex(bad).is_none(), "accepted {bad:?}");
        }
    }

    #[test]
    fn hex_parsing_survives_multibyte_input() {
        // Slicing by byte index would panic here rather than return None.
        assert!(Rgba::parse_hex("#óóóó").is_none());
        assert!(Rgba::parse_hex("źźźźźź").is_none());
    }

    #[test]
    fn the_builtin_theme_is_valid_under_touch() {
        // The fallback has to be clean: a broken file falls back to it, and a
        // fallback that itself needed correcting would hide the original fault.
        let (_, report) = Theme::builtin().sanitised(Pointing::Touch);
        assert!(report.is_clean(), "builtin theme is not clean: {report:?}");
    }

    #[test]
    fn the_builtin_palette_survives_a_16_bit_framebuffer() {
        // The machine with no GPU driver is a deployment target, not an edge
        // case (D-027).
        let p = Palette::default();
        assert_eq!(p.low_contrast_pairs(), Vec::<Neighbours>::new());
        assert_eq!(p.low_contrast_text_pairs(), Vec::<Neighbours>::new());
    }

    #[test]
    fn colours_three_percent_apart_are_caught_at_16_bits() {
        // Two navies a designer would call distinct at 24 bits.
        let p = Palette {
            card: Rgba::rgb(0x1b, 0x2a, 0x44),
            card_active: Rgba::rgb(0x1c, 0x2b, 0x45),
            ..Palette::default()
        };
        assert_eq!(p.low_contrast_pairs(), vec![("card", "card_active")]);
    }

    #[test]
    fn touch_raises_bars_to_the_floor_and_says_so() {
        let m = Metrics {
            top_bar: 20,
            bottom_bar: 24,
            ..Metrics::default()
        };
        let (out, log) = m.sanitised(Pointing::Touch);
        assert_eq!(out.top_bar, MIN_TOUCH_TARGET);
        assert_eq!(out.bottom_bar, MIN_TOUCH_TARGET);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].field, "top_bar");
        assert_eq!((log[0].from, log[0].to), (20, MIN_TOUCH_TARGET));
    }

    #[test]
    fn a_pointer_session_may_have_thinner_bars() {
        // The point of D-030: 48 + 48 is 27% of a 360-unit-high landscape phone.
        let m = Metrics {
            top_bar: 32,
            bottom_bar: 32,
            ..Metrics::default()
        };
        let (out, log) = m.sanitised(Pointing::Pointer);
        assert_eq!((out.top_bar, out.bottom_bar), (32, 32));
        assert!(log.is_empty());
    }

    #[test]
    fn even_a_pointer_session_has_a_floor() {
        let m = Metrics {
            top_bar: 1,
            ..Metrics::default()
        };
        let (out, log) = m.sanitised(Pointing::Pointer);
        assert_eq!(out.top_bar, Pointing::Pointer.floor());
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn negative_sizes_are_corrected_rather_than_propagated() {
        let m = Metrics {
            tile_gap: -10,
            inner_gap: -1,
            outer_gap: -1,
            focus_width: 0,
            ..Metrics::default()
        };
        let (out, log) = m.sanitised(Pointing::Touch);
        assert_eq!((out.tile_gap, out.inner_gap, out.outer_gap), (0, 0, 0));
        assert_eq!(out.focus_width, 1);
        assert_eq!(log.len(), 4);
        // A negative gap would otherwise reach `tile()` and cut a tile larger
        // than the area it came from.
        assert!(out.gaps().inner >= 0 && out.gaps().outer >= 0);
    }

    #[test]
    fn zero_gaps_are_left_alone() {
        let m = Metrics {
            inner_gap: 0,
            outer_gap: 0,
            tile_gap: 0,
            ..Metrics::default()
        };
        let (out, log) = m.sanitised(Pointing::Touch);
        assert!(log.is_empty());
        assert_eq!(out.gaps(), Gaps { outer: 0, inner: 0 });
    }

    #[test]
    fn unreadable_font_sizes_are_raised() {
        let f = Fonts {
            size_bar: 0,
            size_tile: -4,
            ..Fonts::default()
        };
        let (out, log) = f.sanitised();
        assert_eq!(out.size_bar, Fonts::MIN_SIZE);
        assert_eq!(out.size_tile, Fonts::MIN_SIZE);
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn sanitising_is_idempotent() {
        // Applying a correction twice must not drift: config is loaded, saved
        // and loaded again, and a value that moved each time would creep.
        let messy = Theme {
            metrics: Metrics {
                top_bar: 3,
                tile_gap: -5,
                ..Metrics::default()
            },
            fonts: Fonts {
                size_bar: 1,
                ..Fonts::default()
            },
            ..Theme::builtin()
        };
        let (once, _) = messy.sanitised(Pointing::Touch);
        let (twice, report) = once.clone().sanitised(Pointing::Touch);
        assert_eq!(once, twice);
        assert!(report.is_clean());
    }

    #[test]
    fn metrics_feed_the_existing_layout_types() {
        // The theme is the single source of these numbers; `zones` and `tile`
        // must not keep their own.
        let m = Metrics::default();
        assert_eq!(
            m.bar_heights(),
            BarHeights {
                top: m.top_bar,
                bottom: m.bottom_bar
            }
        );
        let z = crate::shell::zones(crate::Rect::new(0, 0, 1920, 1080), m.bar_heights());
        assert!(z.covers(crate::Rect::new(0, 0, 1920, 1080)));
    }
}
