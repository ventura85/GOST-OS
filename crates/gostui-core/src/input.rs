//! Input as logic: what a point on the screen means, and what a key combination
//! does.
//!
//! Both questions are answered here rather than in the compositor, and for the
//! same reason the window model is (D-016): "a click at (x, y) focuses the second
//! tile" and "Super+Tab moves focus on" are statements `cargo test` can check,
//! while "the compositor routed the event correctly" is something somebody has to
//! sit and watch.
//!
//! # Why keys are numbers here and not `xkbcommon::Keysym`
//!
//! The compositor needs libxkbcommon — it is what turns a scancode into a symbol
//! under the user's own keyboard layout, and writing that ourselves would be
//! inventing something standardised (a Polish layout must stay a Polish layout).
//! Core does not, and must not: a crate that pulls in a system C library is a
//! crate that stops building on the next platform. So the boundary is a plain
//! number.
//!
//! The numbering *is* xkb's (the X11 keysym values), so the translation on the
//! compositor's side is `Keysym(raw.raw())` and not a lookup table nobody would
//! keep up to date. Borrowing the numbering is not the same as depending on the
//! library.
//!
//! # What is deliberately not here
//!
//! Pointer acceleration, tap-to-click, gesture recognition — the pointer mode of
//! D-022. That is a mode the user switches on, it needs a control in the UI that
//! does not exist yet, and it is M3. What is here is the part that must be right
//! *before* it: touch, pointer and keyboard are three separate paths, and nothing
//! in this module tempts anyone to make touch a renamed pointer (D-020).

use crate::geometry::Point;
use crate::shell::{bottom_bar_layout, card_columns, layout_tiles, top_bar_layout, Zones};
use crate::tab::TabStrip;
use crate::theme::Metrics;
use crate::window::{Placed, WindowId};

/// Keyboard modifiers held down when a key was pressed.
///
/// A bit set rather than four booleans, because bindings are compared for
/// equality and four-field equality is four chances to forget a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1 << 0);
    pub const CTRL: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);
    /// The Super / Windows / Command key. The shell's own modifier: a binding
    /// that carries it is ours, everything else belongs to the application.
    pub const LOGO: Self = Self(1 << 3);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Build a set from the four flags the compositor reads off the xkb state.
    pub const fn from_flags(shift: bool, ctrl: bool, alt: bool, logo: bool) -> Self {
        let mut bits = 0;
        if shift {
            bits |= Self::SHIFT.0;
        }
        if ctrl {
            bits |= Self::CTRL.0;
        }
        if alt {
            bits |= Self::ALT.0;
        }
        if logo {
            bits |= Self::LOGO.0;
        }
        Self(bits)
    }
}

impl std::ops::BitOr for Mods {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A key symbol in the X11 / xkb numbering.
///
/// See the module docs for why the numbering is borrowed but the library is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keysym(pub u32);

impl Keysym {
    pub const TAB: Self = Self(0xff09);
    pub const ESCAPE: Self = Self(0xff1b);
    pub const LEFT: Self = Self(0xff51);
    pub const RIGHT: Self = Self(0xff53);
    /// Lower-case `q`. xkb reports the symbol *after* the layout has been
    /// applied, so on a Polish layout this is still the key marked Q.
    pub const Q: Self = Self(0x071);
    /// Lower-case `f`.
    pub const F: Self = Self(0x066);
}

/// Something the shell does in answer to a key combination.
///
/// Only what the shell can actually do today. An action for a feature that does
/// not exist yet would be a binding that silently swallows a key the application
/// was waiting for, which is worse than not having the binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Move focus to the next window in bottom-bar order, pulling it into a tile
    /// if it was waiting.
    FocusNextWindow,
    FocusPreviousWindow,
    /// Ask the focused window to close. The client may refuse — that is its
    /// right, and "ask" is the whole of the protocol's vocabulary here.
    CloseWindow,
    /// Put the focused window in or out of fullscreen.
    ///
    /// The shell's own, not a request forwarded to the client, and that is the
    /// point: a fullscreen window is the only one allowed to cover both bars, so
    /// the way out of it must not depend on the application still answering.
    ToggleFullscreen,
}

/// A key with its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Binding {
    pub key: Keysym,
    pub mods: Mods,
}

impl Binding {
    pub const fn new(key: Keysym, mods: Mods) -> Self {
        Self { key, mods }
    }
}

/// The shell's key bindings.
///
/// A list rather than a map: it holds single digits of entries, it has to keep
/// the order the user wrote it in when it comes from configuration, and a linear
/// scan of ten items per keypress is not a cost worth a hash table.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<(Binding, Action)>,
}

impl Default for Keymap {
    /// Super and nothing else.
    ///
    /// Every binding carries [`Mods::LOGO`], and that is the rule, not the
    /// current contents: a compositor that claims Alt+Tab or a bare F-key takes
    /// it away from every application that wanted it. The shell owns one
    /// modifier and applications own the rest of the keyboard.
    fn default() -> Self {
        let mut map = Self {
            bindings: Vec::new(),
        };
        map.bind(
            Binding::new(Keysym::TAB, Mods::LOGO),
            Action::FocusNextWindow,
        );
        map.bind(
            Binding::new(Keysym::TAB, Mods::LOGO | Mods::SHIFT),
            Action::FocusPreviousWindow,
        );
        map.bind(Binding::new(Keysym::Q, Mods::LOGO), Action::CloseWindow);
        map.bind(
            Binding::new(Keysym::F, Mods::LOGO),
            Action::ToggleFullscreen,
        );
        map
    }
}

impl Keymap {
    /// Add a binding, replacing any earlier one for the same combination.
    pub fn bind(&mut self, binding: Binding, action: Action) {
        match self.bindings.iter_mut().find(|(b, _)| *b == binding) {
            Some(entry) => entry.1 = action,
            None => self.bindings.push((binding, action)),
        }
    }

    /// What this key press means to the shell, if anything.
    ///
    /// Modifiers must match **exactly**. Ctrl+Super+Tab is not Super+Tab: an
    /// inexact match would have the shell eat combinations it was never given,
    /// and the application on the other side has no way to tell that happened.
    pub fn action(&self, key: Keysym, mods: Mods) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(b, _)| b.key == key && b.mods == mods)
            .map(|(_, a)| *a)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Binding, Action)> + '_ {
        self.bindings.iter().copied()
    }
}

/// An element of the top bar, identified by where it was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopBarItem {
    Menu,
    Search,
    Clock,
    Status,
    /// The bar itself, between the elements. Hit and consumed, never passed on:
    /// the bars are system space and a click there must not reach an application
    /// (the three zones do not overlap, and neither does what they receive).
    Empty,
}

/// What is under a point on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    TopBar(TopBarItem),
    /// A client window, with the position **relative to that window's top-left**
    /// — which is the coordinate the protocol wants, and the one the compositor
    /// would otherwise recompute by hand at every call site.
    Window {
        window: WindowId,
        local: Point,
    },
    /// A card column of the middle zone, by its index **in the tab strip** —
    /// not its position on screen, so the answer survives scrolling and is the
    /// index [`TabStrip::set_active`](crate::tab::TabStrip::set_active) wants.
    Card(usize),
    /// A tile inside a card. `card` indexes the strip, `tile` indexes that
    /// card's [`items`](crate::tab::Tab::items).
    CardTile {
        card: usize,
        tile: usize,
    },
    /// The application zone with nothing on it: past the last card, or the gap
    /// between two of them.
    Desktop,
    /// The chip of the `n`-th window in bottom-bar order, i.e. an index into
    /// [`WindowModel::bar`](crate::window::WindowModel::bar).
    Chip(usize),
    /// The bottom bar, past the last chip.
    BottomBar,
}

impl Hit {
    /// Which card this point belongs to, whether or not it landed on a tile.
    ///
    /// A press inside a card activates that card — including a press on one of
    /// its tiles. The first version of this treated a tile as belonging to
    /// nothing, on the reasoning that a tile will one day launch an application
    /// and should not "quietly" switch cards as a side effect. Measured against
    /// a running shell that was simply wrong: tiles cover about a third of a
    /// card and sit at the top of it, where the eye goes, so a third of every
    /// card was dead and the shell felt like it registered clicks at random.
    ///
    /// Launching and activating are not in competition. `hit_test` still tells
    /// [`Hit::CardTile`] apart from [`Hit::Card`], because launching needs to
    /// know which tile — this is only the question "whose card was that?".
    pub fn card(self) -> Option<usize> {
        match self {
            Self::Card(card) | Self::CardTile { card, .. } => Some(card),
            _ => None,
        }
    }
}

/// Resolve a point on one output to whatever is under it.
///
/// `placed` is that output's windows as [`WindowModel::layout`] returned them —
/// back to front, which is why the search runs backwards: the window drawn last
/// is the one the user is pointing at.
///
/// `chips` is how many windows the bottom bar is showing, not how many exist:
/// the bar drops the ones that do not fit (see [`bottom_bar_layout`]), and a
/// chip that was not drawn cannot be clicked.
///
/// `tabs` and `metrics` are here so the middle zone can be answered for with the
/// **same arithmetic that drew it** ([`card_columns`], [`layout_tiles`]). A card
/// scrolled off the strip cannot be clicked for the same reason a chip that did
/// not fit cannot be: it was never on screen.
///
/// [`WindowModel::layout`]: crate::window::WindowModel::layout
pub fn hit_test(
    zones: &Zones,
    placed: &[Placed],
    chips: usize,
    tabs: &TabStrip,
    metrics: &Metrics,
    point: Point,
) -> Hit {
    if zones.top_bar.contains(point) {
        let l = top_bar_layout(zones.top_bar);
        for (rect, item) in [
            (l.menu, TopBarItem::Menu),
            (l.search, TopBarItem::Search),
            (l.clock, TopBarItem::Clock),
            (l.status, TopBarItem::Status),
        ] {
            if rect.is_some_and(|r| r.contains(point)) {
                return Hit::TopBar(item);
            }
        }
        return Hit::TopBar(TopBarItem::Empty);
    }

    if zones.bottom_bar.contains(point) {
        for (i, chip) in bottom_bar_layout(zones.bottom_bar, chips)
            .iter()
            .enumerate()
        {
            if chip.contains(point) {
                return Hit::Chip(i);
            }
        }
        return Hit::BottomBar;
    }

    for p in placed.iter().rev() {
        if p.rect.contains(point) {
            return Hit::Window {
                window: p.window,
                local: Point::new(point.x - p.rect.x(), point.y - p.rect.y()),
            };
        }
    }

    // Nothing covers the middle zone, so the cards are what is under the point.
    let layout = card_columns(zones.apps, metrics, tabs.len(), tabs.active_index());
    for (n, card) in layout.cards.iter().enumerate() {
        if !card.contains(point) {
            continue;
        }
        let index = layout.first + n;
        let items = tabs.iter().nth(index).map_or(0, |t| t.items.len());
        for (t, tile) in layout_tiles(*card, metrics, items).iter().enumerate() {
            if tile.contains(point) {
                return Hit::CardTile {
                    card: index,
                    tile: t,
                };
            }
        }
        return Hit::Card(index);
    }
    Hit::Desktop
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Rect, Size};
    use crate::layout::{Gaps, Placement, Split};
    use crate::shell::{zones, BarHeights};
    use crate::window::WindowModel;
    use crate::OutputId;

    const MONITOR: Rect = Rect::new(0, 0, 1920, 1080);

    fn monitor() -> Zones {
        zones(MONITOR, BarHeights::default())
    }

    /// [`hit_test`] for the cases that are about bars and windows.
    ///
    /// An empty strip means the middle zone has no cards on it, so those tests
    /// keep asking exactly what they asked before the cards arrived, and
    /// [`Hit::Desktop`] still means "nothing here".
    fn hit(z: &Zones, placed: &[Placed], chips: usize, point: Point) -> Hit {
        hit_test(
            z,
            placed,
            chips,
            &TabStrip::new(),
            &Metrics::default(),
            point,
        )
    }

    /// A strip of `cards` cards, each with `items` shortcuts, the first active.
    fn strip(cards: usize, items: usize) -> TabStrip {
        let mut s = TabStrip::new();
        for c in 0..cards {
            let id = s.add(format!("Karta {c}"));
            let tab = s.get_mut(id).expect("just added");
            for i in 0..items {
                tab.items.push(crate::tab::LauncherItem::new(
                    format!("app{i}"),
                    format!("App {i}"),
                ));
            }
        }
        s
    }

    /// Two tiled windows on one output, laid out exactly as the compositor lays
    /// them out — the point being that the hit test reads the same rectangles the
    /// renderer draws, not a second guess at them.
    fn two_windows() -> (WindowModel, OutputId, Vec<Placed>) {
        let mut model = WindowModel::new();
        let output = OutputId(0);
        model.set_capacity(output, 2);
        model.open_toplevel(output, "foot", "terminal");
        model.open_toplevel(output, "gedit", "notes");
        let placed = model.layout(
            output,
            monitor().apps,
            MONITOR,
            Split::EVEN,
            Gaps::default(),
        );
        (model, output, placed)
    }

    #[test]
    fn a_point_in_a_tile_names_the_window_and_its_local_coordinate() {
        let (_, _, placed) = two_windows();
        let first = placed[0];
        assert_eq!(first.placement, Placement::Tiled);
        let p = Point::new(first.rect.x() + 7, first.rect.y() + 9);
        assert_eq!(
            hit(&monitor(), &placed, 2, p),
            Hit::Window {
                window: first.window,
                local: Point::new(7, 9),
            }
        );
    }

    #[test]
    fn the_gap_between_two_tiles_is_desktop_not_a_window() {
        // The bug this guards: a hit test that snaps to the nearest window makes
        // the gap between tiles focus one of them at random.
        let (_, _, placed) = two_windows();
        let gap_x = (placed[0].rect.right() + placed[1].rect.x()) / 2;
        let p = Point::new(gap_x, placed[0].rect.y() + 10);
        assert_eq!(hit(&monitor(), &placed, 2, p), Hit::Desktop);
    }

    #[test]
    fn the_bars_never_hand_a_point_to_a_window() {
        // The three zones do not overlap, so a click on a bar is the system's
        // even when a window happens to be laid out behind it.
        let z = monitor();
        let full = vec![Placed {
            window: WindowId(0),
            rect: Rect::from_size(Size::new(1920, 1080)),
            placement: Placement::Floating,
        }];
        let top = hit(&z, &full, 0, Point::new(900, 4));
        let bottom = hit(&z, &full, 0, Point::new(900, z.bottom_bar.y() + 4));
        assert!(matches!(top, Hit::TopBar(_)), "{top:?}");
        assert_eq!(bottom, Hit::BottomBar);
    }

    #[test]
    fn a_click_on_the_bottom_bar_names_the_chip_it_landed_on() {
        let z = monitor();
        let chips = crate::shell::bottom_bar_layout(z.bottom_bar, 3);
        for (i, chip) in chips.iter().enumerate() {
            let p = Point::new(chip.x() + 2, chip.y() + 2);
            assert_eq!(hit(&z, &[], 3, p), Hit::Chip(i));
        }
        // Past the last chip is the bar, not the last chip stretched to the edge.
        let past = Point::new(z.bottom_bar.right() - 6, z.bottom_bar.y() + 24);
        assert_eq!(hit(&z, &[], 3, past), Hit::BottomBar);
    }

    #[test]
    fn a_chip_the_bar_had_no_room_for_cannot_be_hit() {
        // The phone bar fits one chip out of six. The other five are unreachable,
        // which is a known limitation (see `bottom_bar_layout`) — but it must not
        // turn into *wrong* indices, which is what a hit test that assumed the
        // full count would produce.
        let z = zones(Rect::from_size(Size::new(360, 800)), BarHeights::default());
        let drawn = crate::shell::bottom_bar_layout(z.bottom_bar, 6).len();
        let mut hits = Vec::new();
        for x in z.bottom_bar.x()..z.bottom_bar.right() {
            if let Hit::Chip(i) = hit(&z, &[], 6, Point::new(x, z.bottom_bar.y() + 24)) {
                if !hits.contains(&i) {
                    hits.push(i);
                }
            }
        }
        assert_eq!(hits.len(), drawn);
        assert!(hits.iter().all(|i| *i < drawn));
    }

    #[test]
    fn the_front_window_wins_when_a_dialog_covers_a_tile() {
        // `layout` returns back to front, so a floating dialog is last — and a
        // click on it must reach the dialog, never the tile underneath.
        let (mut model, output, _) = two_windows();
        let parent = model.bar(output)[0];
        let dialog = model
            .open_child(
                parent,
                crate::layout::SurfaceRole::Dialog,
                Size::new(300, 200),
                "Zapisz jako",
            )
            .expect("dialog opens on an existing parent");
        let placed = model.layout(
            output,
            monitor().apps,
            MONITOR,
            Split::EVEN,
            Gaps::default(),
        );
        let d = placed
            .iter()
            .find(|p| p.window == dialog)
            .expect("the dialog is placed");
        let centre = Point::new(d.rect.x() + d.rect.w() / 2, d.rect.y() + d.rect.h() / 2);
        assert!(
            matches!(hit(&monitor(), &placed, 2, centre), Hit::Window { window, .. } if window == dialog)
        );
    }

    #[test]
    fn top_bar_elements_are_told_apart_from_the_bar_around_them() {
        let z = monitor();
        let l = top_bar_layout(z.top_bar);
        let menu = l.menu.expect("the menu is never dropped");
        assert_eq!(
            hit(&z, &[], 0, Point::new(menu.x() + 1, menu.y() + 1)),
            Hit::TopBar(TopBarItem::Menu)
        );
        // Between the menu and whatever is next: still the bar's, still not a
        // window's.
        assert_eq!(
            hit(&z, &[], 0, Point::new(menu.right() + 2, menu.y() + 1)),
            Hit::TopBar(TopBarItem::Empty)
        );
    }

    #[test]
    fn the_default_keymap_only_claims_combinations_with_super() {
        let map = Keymap::default();
        for (binding, _) in map.iter() {
            assert!(
                binding.mods.contains(Mods::LOGO),
                "{binding:?} takes a key away from applications"
            );
        }
    }

    #[test]
    fn modifiers_must_match_exactly() {
        let map = Keymap::default();
        assert_eq!(
            map.action(Keysym::TAB, Mods::LOGO),
            Some(Action::FocusNextWindow)
        );
        assert_eq!(
            map.action(Keysym::TAB, Mods::LOGO | Mods::SHIFT),
            Some(Action::FocusPreviousWindow)
        );
        // An extra modifier is a different combination, and it belongs to the
        // application.
        assert_eq!(map.action(Keysym::TAB, Mods::LOGO | Mods::CTRL), None);
        assert_eq!(map.action(Keysym::TAB, Mods::NONE), None);
        assert_eq!(map.action(Keysym::Q, Mods::NONE), None);
    }

    #[test]
    fn rebinding_replaces_rather_than_shadows() {
        let mut map = Keymap::default();
        map.bind(Binding::new(Keysym::TAB, Mods::LOGO), Action::CloseWindow);
        assert_eq!(
            map.action(Keysym::TAB, Mods::LOGO),
            Some(Action::CloseWindow)
        );
        assert_eq!(map.iter().count(), 4, "a rebind must not grow the map");
    }

    #[test]
    fn modifier_flags_round_trip() {
        let m = Mods::from_flags(true, false, false, true);
        assert!(m.contains(Mods::SHIFT) && m.contains(Mods::LOGO));
        assert!(!m.contains(Mods::CTRL) && !m.contains(Mods::ALT));
        assert_eq!(m, Mods::SHIFT | Mods::LOGO);
        assert!(Mods::from_flags(false, false, false, false).is_empty());
    }

    #[test]
    fn a_click_lands_on_the_card_that_was_drawn() {
        let (z, m, tabs) = (monitor(), Metrics::default(), strip(5, 0));
        let cards = card_columns(z.apps, &m, 5, 0).cards;
        for (i, card) in cards.iter().enumerate() {
            let p = Point::new(card.x() + 2, card.bottom() - 2);
            assert_eq!(hit_test(&z, &[], 0, &tabs, &m, p), Hit::Card(i));
        }
    }

    #[test]
    fn a_click_on_a_tile_names_the_tile_and_a_click_beside_it_does_not() {
        let (z, m, tabs) = (monitor(), Metrics::default(), strip(3, 4));
        let card = card_columns(z.apps, &m, 3, 0).cards[1];
        let tiles = layout_tiles(card, &m, 4);

        let centre = |r: Rect| Point::new(r.x() + r.w() / 2, r.y() + r.h() / 2);
        assert_eq!(
            hit_test(&z, &[], 0, &tabs, &m, centre(tiles[2])),
            Hit::CardTile { card: 1, tile: 2 }
        );
        // Below the last tile is still the card — the empty space belongs to it
        // and must not read as the shortcut above it.
        let below = Point::new(card.x() + card.w() / 2, card.bottom() - 2);
        assert_eq!(hit_test(&z, &[], 0, &tabs, &m, below), Hit::Card(1));
    }

    #[test]
    fn every_point_of_a_card_belongs_to_that_card_tiles_included() {
        // The regression this exists for: tiles used to belong to no card, so a
        // third of every column — the part with something drawn in it — did
        // nothing when pressed, and the shell looked like it dropped clicks.
        let (z, m, tabs) = (monitor(), Metrics::default(), strip(4, 8));
        let layout = card_columns(z.apps, &m, 4, 0);
        for (n, card) in layout.cards.iter().enumerate() {
            let index = layout.first + n;
            for y in (card.y()..card.bottom()).step_by(17) {
                for x in (card.x()..card.right()).step_by(13) {
                    let hit = hit_test(&z, &[], 0, &tabs, &m, Point::new(x, y));
                    assert_eq!(
                        hit.card(),
                        Some(index),
                        "({x}, {y}) is inside card {index} but reads as {hit:?}"
                    );
                }
            }
        }
        // And nothing outside a card claims to be one.
        let past = Point::new(layout.cards.last().unwrap().right() + 6, z.apps.y() + 40);
        assert_eq!(hit_test(&z, &[], 0, &tabs, &m, past).card(), None);
        assert_eq!(Hit::BottomBar.card(), None);
    }

    #[test]
    fn the_clipped_card_is_clickable_and_answers_with_its_strip_index() {
        // A landscape phone: two cards and a sliver of the third (D-046).
        let z = zones(Rect::new(0, 0, 780, 360), BarHeights::default());
        let (m, tabs) = (Metrics::default(), strip(6, 0));
        let cards = card_columns(z.apps, &m, 6, 0).cards;
        let sliver = *cards.last().expect("three columns on this output");
        let p = Point::new(sliver.right() - 1, sliver.y() + 1);
        assert_eq!(
            hit_test(&z, &[], 0, &tabs, &m, p),
            Hit::Card(cards.len() - 1)
        );
    }

    #[test]
    fn a_card_scrolled_off_the_strip_cannot_be_clicked() {
        // The same rule the bottom bar already follows: what was not drawn
        // cannot be hit. With card 11 active the strip starts at 5, so the
        // leftmost column is card 5 and cards 0..5 are unreachable.
        let (z, m) = (monitor(), Metrics::default());
        let mut tabs = strip(12, 0);
        let last = tabs.iter().last().expect("twelve cards").id;
        assert!(tabs.set_active(last));

        let l = card_columns(z.apps, &m, 12, tabs.active_index());
        assert_eq!(l.first, 5);
        let p = Point::new(l.cards[0].x() + 2, l.cards[0].y() + 2);
        assert_eq!(hit_test(&z, &[], 0, &tabs, &m, p), Hit::Card(5));
    }

    #[test]
    fn a_window_still_wins_over_the_cards_under_it() {
        // Z-order is one list (paint.rs) and the hit test has to read it the
        // same way round: windows sit on the cards, not beside them.
        let (m, tabs) = (Metrics::default(), strip(5, 4));
        let (_, _, placed) = two_windows();
        let r = placed[0].rect;
        let p = Point::new(r.x() + r.w() / 2, r.y() + r.h() / 2);
        assert!(matches!(
            hit_test(&monitor(), &placed, 2, &tabs, &m, p),
            Hit::Window { window, .. } if window == placed[0].window
        ));
    }
}
