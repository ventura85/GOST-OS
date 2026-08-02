//! Painting the shell itself: the two bars and the tab slider.
//!
//! No text yet — `cosmic-text` arrives with step 5 of M1 (D-005). Until then every
//! label is drawn as the block of space it will occupy, which is deliberately
//! useful: it shows whether the *layout* is right before any font question is
//! allowed to confuse the picture.
//!
//! **The shape of this module matters more than what it draws.** Nothing here
//! touches pixels: [`display_list`] turns the shell state into a sequence of
//! filled rectangles in logical units, and *that* is what a renderer executes —
//! the CPU rasteriser by writing bytes, GLES2 by issuing `draw_solid`. Both
//! paths are first-class (D-027), and the only way to keep them from drifting
//! apart is to give them one description of the picture to share (D-028).

use crate::text::{Align, Primitive, SurfaceSlot, TextRenderer, TextRun};
use crate::{Canvas, Rgba};
use gostui_core::geometry::Rect;
use gostui_core::shell::Zones;
use gostui_core::tab::TabStrip;
use gostui_core::theme::Theme;

/// The colour roles, re-exported from the theme.
///
/// The painter does not own the palette and must not grow a second copy of it:
/// a colour the user can change lives in `gostui-core` with the rest of the
/// theme (D-032). The default is still the GOST navy/cyan/lime.
pub use gostui_core::theme::Palette;

/// Everything the painter needs. Borrowed, never owned: the shell state lives in
/// `gostui-core` and the renderer only reads it.
#[derive(Debug)]
pub struct ShellView<'a> {
    pub zones: Zones,
    pub tabs: &'a TabStrip,
    /// Open windows, as they appear in the bottom bar. Only the count and order
    /// are drawn so far; the labels arrive when the bottom bar gets text.
    pub windows: &'a [String],
    pub focused_window: Option<usize>,
    /// Where the client windows go, back to front, as `gostui_core`'s window
    /// model placed them. Empty until clients exist — the `--png` diagnostic and
    /// the golden images draw the shell with no windows on purpose, because a
    /// client's pixels are not ours to reproduce.
    pub surfaces: &'a [SurfaceSlot],
    /// What the clock says, already formatted by `gostui_core::clock`.
    ///
    /// A string rather than a time, because deciding what the clock reads is
    /// logic with a testable answer and belongs in core (D-016). By the time it
    /// reaches the painter the only question left is where to put it.
    pub clock: Option<&'a str>,
}

// Sizes of the *old* card slider, deliberately still constants.
//
// D-032 puts every size in `Metrics`, and these are the exception that proves
// it: they belong to the floating-card layout that D-031 replaces with a tab
// strip and a tile board. Wiring them to the theme now would be work spent on a
// picture that is going away. When the D-031 board is built it reads
// `Metrics::tile_unit` and `tile_gap`, and these disappear rather than moving.
const CARD_W: i32 = 260;
const CARD_GAP: i32 = 24;
const TILE: i32 = 56;
const TILE_GAP: i32 = 16;

/// One filled rectangle, in **logical** units.
///
/// Deliberately the smallest possible vocabulary. Everything the shell draws so
/// far is a rectangle of one colour, and keeping it that way means the GLES2 and
/// CPU paths cannot disagree about anything except how they fill it. Text and
/// icons will each add exactly one variant here, and both renderers will have to
/// answer for it at the same time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fill {
    pub rect: Rect,
    pub colour: Rgba,
}

/// Turn the shell state into the list of rectangles that make up the picture.
///
/// Pure: no canvas, no GPU, no allocation beyond the list itself. This is the
/// function golden tests and both renderers share.
pub fn display_list(view: &ShellView<'_>, theme: &Theme) -> Vec<Primitive> {
    let p = &theme.palette;
    let mut out = Vec::new();
    // The background comes first and covers everything, so a renderer that only
    // executes the list needs no separate "clear" concept.
    let z = &view.zones;
    let screen = Rect::new(
        z.top_bar.x(),
        z.top_bar.y(),
        z.top_bar.w(),
        z.bottom_bar.bottom() - z.top_bar.y(),
    );
    push(&mut out, screen, p.desktop);

    // Order is z-order, and this is the whole of it: the tab slider is the
    // desktop, client windows sit on top of it, and the two bars sit on top of
    // everything. A window that could cover the bars would cover the only way
    // out of it.
    slider(&mut out, z.apps, view.tabs, p);
    for slot in view.surfaces.iter().filter(|s| !s.over_bars) {
        out.push(Primitive::Surface(*slot));
    }
    top_bar(&mut out, z.top_bar, view, theme);
    bottom_bar(&mut out, z.bottom_bar, view, p);
    // Last, and only ever a fullscreen window: the one surface entitled to cover
    // the bars. Keeping it here rather than giving the bars a "hidden" flag means
    // the exception is a position in one list, not a second state to keep true.
    for slot in view.surfaces.iter().filter(|s| s.over_bars) {
        out.push(Primitive::Surface(*slot));
    }
    out
}

/// Paint the whole shell into `canvas`.
///
/// Takes the text renderer because resolving a string to pixels needs a font
/// system, and there must be exactly one of those: it holds the font database
/// and the glyph cache, and a second copy would double both.
pub fn paint(
    canvas: &mut Canvas,
    view: &ShellView<'_>,
    theme: &Theme,
    text: &mut TextRenderer,
    scale: i32,
) {
    let list = display_list(view, theme);
    let painted = text.resolve(&list, scale);
    canvas.paint_list(&painted);
}

fn push(out: &mut Vec<Primitive>, rect: Rect, colour: Rgba) {
    // Degenerate rectangles are dropped here rather than in each renderer: a
    // GLES draw call with zero area is a wasted state change, and a fill of
    // negative width is a bug that should not reach a driver at all.
    if rect.w() > 0 && rect.h() > 0 {
        out.push(Primitive::Fill(Fill { rect, colour }));
    }
}

/// An outline `t` units wide, drawn inside `rect` — four fills, no new concept.
fn push_outline(out: &mut Vec<Primitive>, rect: Rect, t: i32, colour: Rgba) {
    let t = t.max(1);
    push(out, Rect::new(rect.x(), rect.y(), rect.w(), t), colour);
    push(
        out,
        Rect::new(rect.x(), rect.bottom() - t, rect.w(), t),
        colour,
    );
    push(out, Rect::new(rect.x(), rect.y(), t, rect.h()), colour);
    push(
        out,
        Rect::new(rect.right() - t, rect.y(), t, rect.h()),
        colour,
    );
}

fn top_bar(out: &mut Vec<Primitive>, bar: Rect, view: &ShellView<'_>, t: &Theme) {
    let p = &t.palette;
    push(out, bar, p.bar);
    // A one-unit rule under the bar: the zones must read as separate at a glance,
    // which is the entire point of having three of them.
    push(
        out,
        Rect::new(bar.x(), bar.bottom() - 1, bar.w(), 1),
        p.bar_edge,
    );

    // Positions come from gostui-core, not from here: which elements fit on a
    // narrow bar is layout arithmetic, and layout arithmetic is testable logic
    // (D-016). The renderer only fills the rectangles it is handed.
    let l = gostui_core::shell::top_bar_layout(bar);
    // The Start Menu is accented because it is the anchor of the bar.
    if let Some(r) = l.menu {
        push(out, r, p.accent);
    }
    for r in [l.search, l.clock, l.status].into_iter().flatten() {
        push(out, r, p.chip);
    }

    // The clock is the first real text in the shell (D-005). It goes in the box
    // core reserved for it, centred, and never gets to decide its own place:
    // a clock that resizes its chip makes the whole bar twitch at :00.
    if let (Some(area), Some(now)) = (l.clock, view.clock) {
        out.push(Primitive::Text(TextRun {
            area,
            text: now.to_string(),
            size: t.fonts.size_bar,
            colour: p.text,
            family: t.fonts.ui.clone(),
            align: Align::Centre,
        }));
    }
}

fn slider(out: &mut Vec<Primitive>, area: Rect, tabs: &TabStrip, p: &Palette) {
    let cards: Vec<_> = tabs.iter().collect();
    if cards.is_empty() || area.h() <= 0 {
        return;
    }

    // Cards take a fixed share of the height rather than a fixed number of units,
    // so they neither rattle around a 1080p screen nor overflow a phone.
    let card_h = (area.h() * 3 / 5).clamp(200, 560);
    let total_w = cards.len() as i32 * CARD_W + (cards.len() as i32 - 1) * CARD_GAP;
    let mut x = area.x() + (area.w() - total_w) / 2;
    let y = area.y() + (area.h() - card_h) / 2;

    for (i, _tab) in cards.iter().enumerate() {
        let card = Rect::new(x, y, CARD_W, card_h);
        let active = i == tabs.active_index();
        push(out, card, if active { p.card_active } else { p.card });
        if active {
            push_outline(out, card, 2, p.accent);
        }
        // Title strip of the card.
        push(
            out,
            Rect::new(card.x() + 20, card.y() + 20, CARD_W - 40, 20),
            p.chip,
        );
        // Launcher grid, three columns (D-008).
        for (n, _) in (0..6).enumerate() {
            let col = (n as i32) % 3;
            let row = (n as i32) / 3;
            let tile = Rect::new(
                card.x() + 20 + col * (TILE + TILE_GAP),
                card.y() + 64 + row * (TILE + TILE_GAP),
                TILE,
                TILE,
            );
            if tile.bottom() <= card.bottom() - 20 {
                push(out, tile, p.tile);
            }
        }
        x += CARD_W + CARD_GAP;
    }
}

fn bottom_bar(out: &mut Vec<Primitive>, bar: Rect, view: &ShellView<'_>, p: &Palette) {
    push(out, bar, p.bar);
    push(out, Rect::new(bar.x(), bar.y(), bar.w(), 1), p.bar_edge);

    // Chip positions come from core, like the top bar's do, and for a reason
    // that only shows up with input: a click has to land on the chip that was
    // drawn (`gostui_core::input::hit_test` reads this same function).
    for (i, chip) in gostui_core::shell::bottom_bar_layout(bar, view.windows.len())
        .into_iter()
        .enumerate()
    {
        push(out, chip, p.chip);
        if view.focused_window == Some(i) {
            // The focused window gets the second brand colour, so focus is
            // readable without relying on a hover state (D-020).
            push(
                out,
                Rect::new(chip.x(), chip.bottom() - 3, chip.w(), 3),
                p.accent_alt,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gostui_core::shell::{zones, BarHeights};
    use gostui_core::theme::Theme;

    /// The picture without text.
    ///
    /// Every golden test here compares pixels, and which font a family resolves
    /// to comes from the machine's fontconfig — so the deterministic tests draw
    /// the shell with no clock, and text is covered by its own tests in
    /// `crate::text`. Losing that coverage is the price of not committing a
    /// font to the repository.
    fn theme_fixture() -> Theme {
        Theme::builtin()
    }

    fn view_fixture() -> (TabStrip, Vec<String>) {
        let mut tabs = TabStrip::new();
        for n in ["Pliki", "Praca", "Rozrywka"] {
            tabs.add(n);
        }
        let windows = vec!["Terminal".to_string(), "Firefox".to_string()];
        (tabs, windows)
    }

    fn list_for(w: i32, h: i32, tabs: &TabStrip, windows: &[String]) -> Vec<Fill> {
        let area = Rect::new(0, 0, w, h);
        let view = ShellView {
            zones: zones(area, BarHeights::default()),
            tabs,
            windows,
            focused_window: Some(0),
            clock: None,
            surfaces: &[],
        };
        crate::text::only_fills(&display_list(&view, &theme_fixture()))
    }

    #[test]
    fn a_window_is_drawn_over_the_desktop_and_under_both_bars() {
        // Z-order is the order of this list and nothing else. A window that
        // could cover the bars would cover the only way out of it — the bottom
        // bar is how you reach every other window (D-025).
        let (tabs, windows) = view_fixture();
        let area = Rect::new(0, 0, 1920, 1080);
        let z = zones(area, BarHeights::default());
        let slot = SurfaceSlot {
            id: 0,
            rect: z.apps,
            src: (0, 0),
            over_bars: false,
        };
        let view = ShellView {
            zones: z,
            tabs: &tabs,
            windows: &windows,
            focused_window: Some(0),
            clock: None,
            surfaces: std::slice::from_ref(&slot),
        };
        let list = display_list(&view, &theme_fixture());
        let window_at = list
            .iter()
            .position(|p| matches!(p, Primitive::Surface(s) if *s == slot))
            .expect("the window is in the list");

        // The desktop background is entry zero, so anything drawn before the
        // window is the desktop layer and anything after it is a bar.
        let after: Vec<Rect> = list[window_at + 1..]
            .iter()
            .filter_map(|p| match p {
                Primitive::Fill(f) => Some(f.rect),
                _ => None,
            })
            .collect();
        assert!(!after.is_empty(), "the bars are drawn after the window");
        for r in after {
            assert!(
                r.bottom() <= z.top_bar.bottom() || r.y() >= z.bottom_bar.y(),
                "{r:?} is drawn over a window but is not part of a bar"
            );
        }
    }

    #[test]
    fn a_fullscreen_window_is_the_one_thing_drawn_over_the_bars() {
        // The exception to the rule above, and the only one. A film with a bar
        // across it is not fullscreen — so this surface goes last, after both
        // bars, and nothing else ever does.
        let (tabs, windows) = view_fixture();
        let area = Rect::new(0, 0, 1920, 1080);
        let z = zones(area, BarHeights::default());
        let slots = [
            SurfaceSlot {
                id: 0,
                rect: z.apps,
                src: (0, 0),
                over_bars: false,
            },
            SurfaceSlot {
                id: 1,
                rect: area,
                src: (0, 0),
                over_bars: true,
            },
        ];
        let view = ShellView {
            zones: z,
            tabs: &tabs,
            windows: &windows,
            focused_window: Some(0),
            clock: None,
            surfaces: &slots,
        };
        let list = display_list(&view, &theme_fixture());
        let full = list
            .iter()
            .position(|p| matches!(p, Primitive::Surface(s) if s.id == 1))
            .expect("the fullscreen window is in the list");
        assert_eq!(full, list.len() - 1, "nothing may be drawn over fullscreen");

        // And the ordinary window is still under the bars: one surface escaping
        // must not lift the rest with it.
        let tiled = list
            .iter()
            .position(|p| matches!(p, Primitive::Surface(s) if s.id == 0))
            .expect("the tiled window is in the list");
        let bar_fills = list[tiled + 1..full]
            .iter()
            .filter(|p| matches!(p, Primitive::Fill(_)))
            .count();
        assert!(bar_fills > 0, "the bars still come after a tiled window");
    }

    #[test]
    fn the_display_list_covers_the_screen_before_anything_else() {
        // Renderers execute the list and nothing else, so if the first entry
        // does not cover the screen, the GLES path shows last frame's rubbish
        // wherever the shell happens not to draw.
        let (tabs, windows) = view_fixture();
        let list = list_for(1920, 1080, &tabs, &windows);
        let first = list.first().expect("a shell always draws something");
        assert_eq!(first.rect, Rect::new(0, 0, 1920, 1080));
        assert_eq!(first.colour, Palette::default().desktop);
    }

    #[test]
    fn the_display_list_holds_no_degenerate_rectangles() {
        // A zero-area fill is a wasted draw call on the GPU and a bug on the way
        // in. Checked on a phone-sized screen, where the cards overflow.
        let (tabs, windows) = view_fixture();
        for (w, h) in [(1920, 1080), (360, 800), (640, 480), (200, 200)] {
            for fill in list_for(w, h, &tabs, &windows) {
                assert!(
                    fill.rect.w() > 0 && fill.rect.h() > 0,
                    "{w}x{h}: {:?}",
                    fill.rect
                );
            }
        }
    }

    #[test]
    fn the_display_list_is_deterministic() {
        // The property both the golden PNGs and the two renderer paths rest on.
        let (tabs, windows) = view_fixture();
        assert_eq!(
            list_for(1920, 1080, &tabs, &windows),
            list_for(1920, 1080, &tabs, &windows)
        );
    }

    #[test]
    fn painting_is_deterministic() {
        let (tabs, windows) = view_fixture();
        let render = || {
            let area = Rect::new(0, 0, 1920, 1080);
            let view = ShellView {
                zones: zones(area, BarHeights::default()),
                tabs: &tabs,
                windows: &windows,
                focused_window: Some(0),
                clock: None,
                surfaces: &[],
            };
            let mut c = Canvas::new(1920, 1080, 1).unwrap();
            paint(&mut c, &view, &theme_fixture(), &mut TextRenderer::new(), 1);
            c
        };
        assert_eq!(render().pixels(), render().pixels());
    }

    #[test]
    fn a_phone_sized_screen_paints_without_panicking() {
        // 360x800 logical: the cards are wider than the screen. Clipping must
        // handle it rather than an assertion firing.
        let (tabs, windows) = view_fixture();
        let area = Rect::new(0, 0, 360, 800);
        let view = ShellView {
            zones: zones(area, BarHeights::default()),
            tabs: &tabs,
            windows: &windows,
            focused_window: None,
            clock: None,
            surfaces: &[],
        };
        let mut c = Canvas::new(360, 800, 2).unwrap();
        paint(&mut c, &view, &theme_fixture(), &mut TextRenderer::new(), 2);
        assert_eq!((c.width(), c.height()), (720, 1600));
    }

    #[test]
    fn an_empty_shell_paints_the_background_only() {
        let tabs = TabStrip::new();
        let area = Rect::new(0, 0, 640, 480);
        let view = ShellView {
            zones: zones(area, BarHeights::default()),
            tabs: &tabs,
            windows: &[],
            focused_window: None,
            clock: None,
            surfaces: &[],
        };
        let mut c = Canvas::new(640, 480, 1).unwrap();
        paint(&mut c, &view, &theme_fixture(), &mut TextRenderer::new(), 1);
        // Middle of the application area stays desktop-coloured.
        let i = ((240 * 640) + 320) * 4;
        assert_eq!(c.pixels()[i], Palette::default().desktop.0);
    }
}
