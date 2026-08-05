//! Painting the shell itself: the two bars and the tab slider.
//!
//! Text is here now (the clock, every card's name, and the caption of every dead
//! tile), but the rule it arrived under still holds: an element whose picture is not ready is
//! drawn as the block of space it will occupy. A tile's icon is such a block
//! today — the layout can be judged before any icon theme is allowed to confuse
//! the picture.
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
use gostui_core::shell::{
    card_columns, card_header, card_title, layout_tiles, plus_mark, tile_face, Zones,
};
use gostui_core::tab::TabStrip;
use gostui_core::theme::{Fonts, Theme};

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
    cards(&mut out, z.apps, view.tabs, theme);
    for slot in view.surfaces.iter().filter(|s| !s.over_bars) {
        out.push(Primitive::Surface(*slot));
        focus_ring(&mut out, slot, theme);
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

/// A line of text in a box, dropped when there is nothing to set.
///
/// Empty runs are filtered here for the same reason degenerate rectangles are:
/// resolving one costs a shaping call and a cache lookup per frame and produces
/// no pixels. It also keeps a property the golden images rest on — no names
/// means no `Text` in the list at all, rather than a list of runs that happen to
/// draw nothing.
fn push_text(out: &mut Vec<Primitive>, area: Rect, text: &str, size: i32, colour: Rgba, f: &Fonts) {
    if text.is_empty() || area.w() <= 0 || area.h() <= 0 {
        return;
    }
    out.push(Primitive::Text(TextRun {
        area,
        text: text.to_string(),
        size,
        colour,
        family: f.ui.clone(),
        align: Align::Centre,
    }));
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

/// The whole of a window's decoration: a ring around the one that has focus.
///
/// **There is no title bar, and that is a decision, not an omission** (D-025).
/// A window here is never dragged and never resized by its edge, so a strip to
/// grab it by would be decoration in the literal sense — it would cost every
/// tile a slice of height and buy nothing. What a title bar is normally *for*
/// already exists elsewhere: the window's name and the way to reach it live on
/// the bottom bar, which is a touch target on a phone as well as a click target
/// on a monitor.
///
/// Drawn inside the window's own rectangle rather than in the gap between tiles:
/// the gap is a theme setting and can be zero, and a focus ring that disappears
/// when somebody sets `inner_gap = 0` is a focus ring that cannot be relied on.
/// The cost is the outermost two pixels of the client's picture.
fn focus_ring(out: &mut Vec<Primitive>, slot: &SurfaceSlot, t: &Theme) {
    // A fullscreen window covers the screen on purpose; ringing it would put a
    // frame around a film.
    if !slot.focused || slot.over_bars {
        return;
    }
    push_outline(out, slot.rect, t.metrics.focus_width, t.palette.accent);
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
    // The Start Menu is accented because it is the anchor of the bar, and it
    // carries the one mark in the shell that has to be recognised rather than
    // read: four squares. Drawn as four fills of the bar colour punched out of
    // the accented button, so the icon costs four rectangles and no texture —
    // which is the whole argument of D-044, that the look and the display list
    // want the same thing.
    if let Some(r) = l.menu {
        push(out, r, p.accent);
        if let Some(squares) = gostui_core::shell::menu_icon(r) {
            for s in squares {
                push(out, s, p.bar);
            }
        }
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

/// The middle zone: card columns, as many as the output has room for (D-046).
///
/// **Not one rectangle is positioned here.** `card_columns` and `layout_tiles`
/// in core hand over the geometry and this function fills it in — which is the
/// rule for the whole crate (layout is logic, drawing is not), and it is what
/// makes a click land on the card the user can see: `hit_test` reads the same
/// two functions.
fn cards(out: &mut Vec<Primitive>, area: Rect, tabs: &TabStrip, theme: &Theme) {
    let (p, m) = (&theme.palette, &theme.metrics);
    let layout = card_columns(area, m, tabs.len(), tabs.active_index());

    for (n, card) in layout.cards.iter().enumerate() {
        let index = layout.first + n;
        let active = index == tabs.active_index();
        push(out, *card, if active { p.card_active } else { p.card });
        // The header: the bar, then the card's name in it. A card that says
        // nothing is a column of shortcuts with no reason to be a card, and an
        // empty bar over one reads as a heading that failed to load.
        let tab = tabs.iter().nth(index);
        push(out, card_header(*card, m), p.chip);
        if let (Some(area), Some(t)) = (
            card_title(*card, m, Fonts::line_height(theme.fonts.size_card)),
            tab,
        ) {
            push_text(
                out,
                area,
                &t.name,
                theme.fonts.size_card,
                p.text,
                &theme.fonts,
            );
        }
        // A dead tile is an icon and a name (D-033). The icon has no pixels yet
        // — that is its own step, with an icon theme and a cache behind it — so
        // what a tile shows today is its name, cut to the tile by the text stack
        // when the application chose a long one.
        let items = tab.map_or(0, |t| t.items.len());
        let line = Fonts::line_height(theme.fonts.size_tile);
        for (i, tile) in layout_tiles(*card, m, items).iter().enumerate() {
            push(out, *tile, p.tile);
            let face = tile_face(*tile, line);
            let (Some(caption), Some(item)) = (face.caption, tab.and_then(|t| t.items.get(i)))
            else {
                continue;
            };
            push_text(
                out,
                caption,
                &item.name,
                theme.fonts.size_tile,
                p.text,
                &theme.fonts,
            );
        }
        // The focus ring goes **last**, after everything the card contains.
        // Drawn before the header — which is where it was until the header got a
        // name and the gap became visible — the header's fill paints over its
        // top edge, and a rectangle missing one side does not read as a frame.
        if active {
            push_outline(out, *card, m.focus_width, p.accent);
        }
    }

    // The `[+] Nowa karta` slot closes the strip (`gostos.md` §B). Dimmer than a
    // card, because it is not one: it holds nothing and activating it is not
    // what pressing it does. The mark is two rectangles — see `plus_mark`.
    if let Some(slot) = layout.add {
        push(out, slot, p.card);
        push(out, card_header(slot, m), p.chip);
        for bar in plus_mark(slot, m).into_iter().flatten() {
            push(out, bar, p.text_dim);
        }
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

    /// A card of `items` shortcuts, and the display list a monitor draws it into.
    fn card_with_items(items: usize) -> (Vec<Primitive>, Rect, Theme) {
        let mut tabs = TabStrip::new();
        let id = tabs.add("Pliki");
        let tab = tabs.get_mut(id).expect("just added");
        for i in 0..items {
            tab.items.push(gostui_core::tab::LauncherItem::new(
                format!("a{i}"),
                "Nazwa",
            ));
        }
        let theme = theme_fixture();
        let z = zones(Rect::new(0, 0, 1920, 1080), BarHeights::default());
        let view = ShellView {
            zones: z,
            tabs: &tabs,
            windows: &[],
            focused_window: None,
            clock: None,
            surfaces: &[],
        };
        let card = card_columns(z.apps, &theme.metrics, 1, 0).cards[0];
        (display_list(&view, &theme), card, theme)
    }

    #[test]
    fn a_caption_is_drawn_for_every_tile_that_was_drawn_and_for_no_other() {
        // Ninety shortcuts on a card with room for a dozen. Captions have to stop
        // where the tiles stop: a name for a tile that was dropped would be text
        // floating on the card below the grid, pointing at nothing. The tile
        // count is not written down here on purpose — it comes from the same
        // function the painter used, so this stays true when the metrics change.
        let (list, card, theme) = card_with_items(90);
        let tiles = layout_tiles(card, &theme.metrics, 90);
        let captions: Vec<&TextRun> = list
            .iter()
            .filter_map(|p| match p {
                Primitive::Text(r) if r.text == "Nazwa" => Some(r),
                _ => None,
            })
            .collect();

        assert!(!tiles.is_empty(), "the fixture must draw some tiles");
        assert_eq!(captions.len(), tiles.len());
        for (caption, tile) in captions.iter().zip(&tiles) {
            let expected = tile_face(*tile, Fonts::line_height(theme.fonts.size_tile))
                .caption
                .expect("a full-size tile has room for a name");
            assert_eq!(
                caption.area, expected,
                "the caption belongs to its own tile"
            );
        }
    }

    #[test]
    fn the_focus_ring_is_drawn_over_the_header_and_not_under_it() {
        // Found by looking, not by testing: the ring was pushed straight after
        // the card fill, so the header's own fill painted over its top edge and
        // the active card wore a frame with three sides. Nobody noticed while
        // the header was an anonymous bar; the name made it a thing you look at.
        let (list, _, theme) = card_with_items(4);
        let fills: Vec<(usize, &Fill)> = list
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p {
                Primitive::Fill(f) => Some((i, f)),
                _ => None,
            })
            .collect();
        let header = fills
            .iter()
            .find(|(_, f)| f.colour == theme.palette.chip)
            .expect("the header bar")
            .0;
        let ring = fills
            .iter()
            .find(|(_, f)| f.colour == theme.palette.accent)
            .expect("the focus ring")
            .0;
        assert!(ring > header, "ring at {ring}, header at {header}");
    }

    #[test]
    fn nothing_named_means_no_text_in_the_middle_zone_at_all() {
        // What the golden images rest on: with nothing named, the middle zone
        // contains no glyphs, so those files stay identical on this station and
        // in CI. If text ever appeared for an unnamed card, they would start
        // disagreeing for a reason that has nothing to do with the shell.
        //
        // The zone has **two** sources of text now — the card's name and each
        // tile's caption — so this asks about both, and a third one added later
        // has to be added here too.
        let mut tabs = TabStrip::new();
        let id = tabs.add("");
        let tab = tabs.get_mut(id).expect("just added");
        for i in 0..4 {
            tab.items
                .push(gostui_core::tab::LauncherItem::new(format!("a{i}"), ""));
        }
        let theme = theme_fixture();
        let z = zones(Rect::new(0, 0, 1920, 1080), BarHeights::default());
        let view = ShellView {
            zones: z,
            tabs: &tabs,
            windows: &[],
            focused_window: None,
            clock: None,
            surfaces: &[],
        };
        let list = display_list(&view, &theme);
        assert!(!list.iter().any(|p| matches!(p, Primitive::Text(_))));

        // And the geometry it guards is really there: the same scene with names
        // draws them, so the images pin down the layout that ships rather than
        // one where the header happens to be skipped.
        let (named, _, _) = card_with_items(4);
        assert_eq!(
            named
                .iter()
                .filter(|p| matches!(p, Primitive::Text(_)))
                .count(),
            5,
            "the card's name plus one caption per tile"
        );
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
            focused: false,
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
    fn the_focused_window_gets_a_ring_and_the_others_get_nothing() {
        // The whole of our decoration. A window is never dragged (D-025), so it
        // needs no strip to be dragged by — and the tile keeps the height a title
        // bar would have taken.
        let (tabs, windows) = view_fixture();
        let z = zones(Rect::new(0, 0, 1920, 1080), BarHeights::default());
        let theme = theme_fixture();
        let slots = [
            SurfaceSlot {
                id: 0,
                rect: Rect::new(0, 48, 960, 984),
                src: (0, 0),
                focused: true,
                over_bars: false,
            },
            SurfaceSlot {
                id: 1,
                rect: Rect::new(964, 48, 956, 984),
                src: (0, 0),
                focused: false,
                over_bars: false,
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
        let list = display_list(&view, &theme);
        let accents: Vec<Rect> = list
            .iter()
            .filter_map(|p| match p {
                Primitive::Fill(f) if f.colour == theme.palette.accent => Some(f.rect),
                _ => None,
            })
            .collect();

        // The four edges are named rather than counted: the slider's active card
        // is drawn in the same accent colour and sits inside the same tile, so a
        // count would count it too.
        let t = theme.metrics.focus_width;
        let r = slots[0].rect;
        for edge in [
            Rect::new(r.x(), r.y(), r.w(), t),
            Rect::new(r.x(), r.bottom() - t, r.w(), t),
            Rect::new(r.x(), r.y(), t, r.h()),
            Rect::new(r.right() - t, r.y(), t, r.h()),
        ] {
            assert!(accents.contains(&edge), "missing ring edge {edge:?}");
        }

        // And nothing of the sort around the window that does not have focus.
        let u = slots[1].rect;
        assert!(
            !accents.contains(&Rect::new(u.x(), u.y(), u.w(), t)),
            "an unfocused window must wear no ring"
        );
    }

    #[test]
    fn a_fullscreen_window_wears_no_ring() {
        let (tabs, windows) = view_fixture();
        let area = Rect::new(0, 0, 1920, 1080);
        let z = zones(area, BarHeights::default());
        let theme = theme_fixture();
        let slots = [SurfaceSlot {
            id: 0,
            rect: area,
            src: (0, 0),
            focused: true,
            over_bars: true,
        }];
        let view = ShellView {
            zones: z,
            tabs: &tabs,
            windows: &windows,
            focused_window: Some(0),
            clock: None,
            surfaces: &slots,
        };
        let list = display_list(&view, &theme);
        // Nothing at all after the surface: a frame around a film is not a frame
        // anybody asked for.
        let surface_at = list
            .iter()
            .position(|p| matches!(p, Primitive::Surface(_)))
            .expect("the window is in the list");
        assert_eq!(surface_at, list.len() - 1);
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
                focused: false,
                over_bars: false,
            },
            SurfaceSlot {
                id: 1,
                rect: area,
                src: (0, 0),
                focused: false,
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
    fn an_empty_shell_still_offers_a_way_to_make_a_card() {
        // It used to paint the background and nothing else. A session with no
        // cards is exactly where being shown how to make one is worth most, so
        // the `[+]` slot is there — alone, and therefore centred.
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
        let at = |x: usize, y: usize| c.pixels()[((y * 640) + x) * 4];
        // Middle of the application area: the slot, one card wide.
        assert_eq!(at(320, 240), Palette::default().card.0);
        // Either side of it, still the desktop — the slot is a column, not a
        // panel that grew to fill the zone.
        assert_eq!(at(40, 240), Palette::default().desktop.0);
        assert_eq!(at(600, 240), Palette::default().desktop.0);
    }
}
