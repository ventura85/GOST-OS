//! The tab slider that replaces the desktop (D-003, Model A).
//!
//! **A `Tab` holds no reference to any window, and it never will.** Tabs are the
//! desktop layer; windows live above them and are switched from the bottom bar.
//! The two are completely disjoint. That is what makes this entire module testable
//! with `cargo test` and no compositor (D-016) — and the property is worth
//! defending in review, because "just let a tab remember its windows" is the
//! obvious-looking change that would destroy it.
//!
//! A tab's content is a grid of launcher shortcuts (D-008).

/// Stable handle for a tab. Survives reordering, unlike an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TabId(pub u32);

/// One shortcut in a tab's grid.
///
/// `desktop_id` is the freedesktop application id (the `.desktop` file's basename,
/// e.g. `org.gnome.Nautilus`). Resolving it to a command line is the job of
/// `gostui-desktop-entry`; this crate stays free of I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherItem {
    pub desktop_id: String,
    /// Display name, already localised by whoever built the item.
    pub name: String,
    /// Icon name for freedesktop icon-theme lookup, not a path.
    pub icon: Option<String>,
}

impl LauncherItem {
    pub fn new(desktop_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            desktop_id: desktop_id.into(),
            name: name.into(),
            icon: None,
        }
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

/// A single themed tab: a name and a grid of shortcuts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    pub name: String,
    pub items: Vec<LauncherItem>,
}

impl Tab {
    pub fn new(id: TabId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            items: Vec::new(),
        }
    }
}

/// A tab that was removed, kept so the removal can be taken back (D-048).
///
/// Deletion does not ask, and that is only defensible because it is reversible —
/// so this is not a convenience, it is the other half of the decision.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Removed {
    tab: Tab,
    /// Where it was, so undo puts it back where the user left it rather than at
    /// the end. A card restored somewhere else is a second surprise on top of
    /// the one being undone.
    index: usize,
    /// Whether it was the pinned one (D-009). One bool, and without it undo
    /// would quietly do less than it says.
    pinned: bool,
}

/// The ordered strip of tabs, plus which one is showing.
#[derive(Debug, Clone, Default)]
pub struct TabStrip {
    tabs: Vec<Tab>,
    active: usize,
    pinned: Option<TabId>,
    next_id: u32,
    /// The last removal, and nothing before it. One level, because the buffer
    /// exists to catch the mistake the user just made — a stack of them is a
    /// history feature, and a history nobody can see is a leak that grows for
    /// as long as the shell runs (D-039).
    removed: Option<Removed>,
    /// Whether the strip is being **changed** rather than used (D-048).
    ///
    /// Lives here rather than in the compositor because both the painter and the
    /// hit test already hold the strip, so this is one source of truth reaching
    /// both — and what a press means is logic with a testable answer (D-016).
    edit: bool,
}

impl TabStrip {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: impl Into<String>) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;
        self.tabs.push(Tab::new(id, name));
        id
    }

    /// Remove a tab. Keeps the active index in range and drops the pin if the
    /// pinned tab is the one going away.
    ///
    /// The tab goes into the undo buffer, replacing whatever was there. Removing
    /// a tab that does not exist changes nothing — including the buffer, which
    /// must not be emptied by a call that did nothing.
    pub fn remove(&mut self, id: TabId) -> bool {
        let Some(idx) = self.index_of(id) else {
            return false;
        };
        let tab = self.tabs.remove(idx);
        let was_pinned = self.pinned == Some(id);
        if was_pinned {
            self.pinned = None;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
        self.removed = Some(Removed {
            tab,
            index: idx,
            pinned: was_pinned,
        });
        true
    }

    /// Put the last removed tab back where it was, and activate it (D-048).
    ///
    /// Activated because undo is answering "that was a mistake": leaving the
    /// restored card somewhere off screen would make the user hunt for proof
    /// that it worked.
    ///
    /// Answers `false` when there is nothing to take back, so the shortcut costs
    /// no frame when it changes nothing — the same rule the slider follows at the
    /// ends of the strip (D-007, D-027).
    ///
    /// The tab keeps its original [`TabId`]: ids are only ever issued upwards,
    /// and the one being restored was not reissued while it sat in the buffer, so
    /// nothing else can be holding it.
    pub fn restore_removed(&mut self) -> bool {
        let Some(r) = self.removed.take() else {
            return false;
        };
        let index = r.index.min(self.tabs.len());
        let id = r.tab.id;
        self.tabs.insert(index, r.tab);
        if r.pinned {
            self.pinned = Some(id);
        }
        self.active = index;
        true
    }

    /// Whether taking back a removal would do anything.
    pub fn can_restore(&self) -> bool {
        self.removed.is_some()
    }

    /// Whether the strip is in edit mode (D-048).
    pub fn is_editing(&self) -> bool {
        self.edit
    }

    /// Enter or leave edit mode. Returns the mode it ended up in.
    ///
    /// A toggle rather than an enter and a separate leave, because the way out
    /// has to be the way in pressed again: the shell owns only `Super`
    /// combinations (D-041), so a bare `Escape` is not ours to claim.
    pub fn set_editing(&mut self, editing: bool) -> bool {
        self.edit = editing;
        self.edit
    }

    pub fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub fn get(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter()
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, id: TabId) -> bool {
        match self.index_of(id) {
            Some(idx) => {
                self.active = idx;
                true
            }
            None => false,
        }
    }

    /// Move one tab to the right (`Super+Right`, D-007).
    ///
    /// Deliberately does **not** wrap around. Under direct manipulation the strip
    /// behaves like a physical object that follows the finger (D-021), and a
    /// physical strip has ends. Wrapping would make the finger and the content
    /// disagree at the boundary.
    pub fn activate_next(&mut self) -> bool {
        if self.active + 1 < self.tabs.len() {
            self.active += 1;
            true
        } else {
            false
        }
    }

    /// Move one tab to the left (`Super+Left`, D-007). Does not wrap; see
    /// [`activate_next`](TabStrip::activate_next).
    pub fn activate_prev(&mut self) -> bool {
        if self.active > 0 {
            self.active -= 1;
            true
        } else {
            false
        }
    }

    /// Move a tab to a new position in the strip. The active tab stays the same
    /// tab, not the same index.
    pub fn reorder(&mut self, id: TabId, to: usize) -> bool {
        let Some(from) = self.index_of(id) else {
            return false;
        };
        let to = to.min(self.tabs.len().saturating_sub(1));
        if from == to {
            return true;
        }
        let active_id = self.active().map(|t| t.id);
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        if let Some(active_id) = active_id {
            if let Some(idx) = self.index_of(active_id) {
                self.active = idx;
            }
        }
        true
    }

    /// Pin a tab (D-009). At most one tab is pinned; pinning a second replaces
    /// the first. A pinned tab reserves screen space and leaves the slider
    /// rotation while it is pinned.
    pub fn pin(&mut self, id: TabId) -> bool {
        if self.index_of(id).is_none() {
            return false;
        }
        self.pinned = Some(id);
        true
    }

    pub fn unpin(&mut self) {
        self.pinned = None;
    }

    pub fn pinned(&self) -> Option<TabId> {
        self.pinned
    }

    /// The tabs the slider actually cycles through: everything except the pinned
    /// one, which is on screen permanently and would otherwise appear twice.
    pub fn sliding(&self) -> impl Iterator<Item = &Tab> {
        let pinned = self.pinned;
        self.tabs.iter().filter(move |t| Some(t.id) != pinned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_of(names: &[&str]) -> TabStrip {
        let mut s = TabStrip::new();
        for n in names {
            s.add(*n);
        }
        s
    }

    #[test]
    fn navigation_clamps_at_both_ends() {
        let mut s = strip_of(&["Pliki", "Praca", "Rozrywka"]);
        assert!(!s.activate_prev(), "already at the left end");
        assert!(s.activate_next());
        assert!(s.activate_next());
        assert_eq!(s.active_index(), 2);
        assert!(!s.activate_next(), "already at the right end");
    }

    #[test]
    fn reorder_keeps_the_same_tab_active_not_the_same_index() {
        let mut s = strip_of(&["Pliki", "Praca", "Rozrywka"]);
        let rozrywka = s.iter().nth(2).unwrap().id;
        s.set_active(rozrywka);
        s.reorder(rozrywka, 0);
        assert_eq!(s.active().unwrap().name, "Rozrywka");
        assert_eq!(s.active_index(), 0);
    }

    #[test]
    fn removing_the_last_tab_pulls_the_active_index_back() {
        let mut s = strip_of(&["a", "b"]);
        let b = s.iter().nth(1).unwrap().id;
        s.set_active(b);
        assert_eq!(s.active_index(), 1);
        s.remove(b);
        assert_eq!(s.active_index(), 0);
        assert_eq!(s.active().unwrap().name, "a");
    }

    #[test]
    fn emptying_the_strip_leaves_no_active_tab_and_no_panic() {
        let mut s = strip_of(&["only"]);
        let only = s.iter().next().unwrap().id;
        assert!(s.remove(only));
        assert!(s.is_empty());
        assert!(s.active().is_none());
        assert!(!s.activate_next());
        assert!(!s.activate_prev());
    }

    #[test]
    fn at_most_one_tab_is_pinned() {
        let mut s = strip_of(&["a", "b"]);
        let a = s.iter().next().unwrap().id;
        let b = s.iter().nth(1).unwrap().id;
        s.pin(a);
        s.pin(b);
        assert_eq!(s.pinned(), Some(b));
    }

    #[test]
    fn a_pinned_tab_leaves_the_slider_rotation() {
        let mut s = strip_of(&["a", "b", "c"]);
        let b = s.iter().nth(1).unwrap().id;
        s.pin(b);
        let names: Vec<_> = s.sliding().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["a", "c"]);
    }

    #[test]
    fn removing_the_pinned_tab_clears_the_pin() {
        let mut s = strip_of(&["a"]);
        let a = s.iter().next().unwrap().id;
        s.pin(a);
        s.remove(a);
        assert_eq!(s.pinned(), None);
    }

    #[test]
    fn operations_on_unknown_ids_report_false_rather_than_panicking() {
        let mut s = strip_of(&["a"]);
        let ghost = TabId(999);
        assert!(!s.remove(ghost));
        assert!(!s.set_active(ghost));
        assert!(!s.pin(ghost));
        assert!(!s.reorder(ghost, 0));
    }

    #[test]
    fn undo_puts_the_card_back_where_it_was_with_what_it_held() {
        // The other half of "deletion does not ask" (D-048). Restoring at the end
        // of the strip would be a second surprise stacked on the one being taken
        // back, so position is part of what is restored — as is the content, the
        // point of a card being the arrangement of shortcuts on it (D-008).
        let mut s = strip_of(&["a", "b", "c"]);
        let b = s.iter().nth(1).unwrap().id;
        s.get_mut(b)
            .unwrap()
            .items
            .push(LauncherItem::new("foot", "Terminal"));

        assert!(!s.can_restore(), "nothing removed yet");
        assert!(s.remove(b));
        assert!(s.can_restore());
        assert_eq!(
            s.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["a", "c"]
        );

        assert!(s.restore_removed());
        assert_eq!(
            s.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"],
            "back in the middle, not appended"
        );
        let back = s.iter().nth(1).unwrap();
        assert_eq!(back.id, b, "the same card, not a copy with a new id");
        assert_eq!(back.items.len(), 1, "with what was on it");
        assert_eq!(s.active_index(), 1, "and the shell is looking at it");
    }

    #[test]
    fn undo_holds_one_removal_and_reports_when_it_holds_none() {
        // One level on purpose: the buffer catches the mistake just made. A stack
        // is a history feature nobody asked for, and an unbounded one is a leak
        // in a shell that runs for weeks (D-039).
        let mut s = strip_of(&["a", "b"]);
        let (a, b) = (s.iter().next().unwrap().id, s.iter().nth(1).unwrap().id);
        s.remove(a);
        s.remove(b);
        assert!(s.is_empty());

        assert!(s.restore_removed());
        assert_eq!(
            s.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["b"],
            "the last removal, not the first"
        );
        // And the buffer is spent, so pressing undo again costs no frame.
        assert!(!s.can_restore());
        assert!(!s.restore_removed());
    }

    #[test]
    fn a_removal_that_did_not_happen_does_not_empty_the_undo_buffer() {
        // The trap: `remove` returning early for an unknown id, but only after
        // it has already overwritten the buffer. Undo would then answer "nothing
        // to take back" immediately after a real deletion.
        let mut s = strip_of(&["a", "b"]);
        let a = s.iter().next().unwrap().id;
        assert!(s.remove(a));
        assert!(!s.remove(TabId(999)));
        assert!(s.can_restore(), "the real removal is still there");
        assert!(s.restore_removed());
        assert_eq!(s.iter().next().unwrap().name, "a");
    }

    #[test]
    fn undo_restores_the_pin_it_dropped() {
        // `remove` drops the pin of the tab going away (D-009). An undo that
        // leaves it dropped is an undo that quietly does less than it says.
        let mut s = strip_of(&["a", "b"]);
        let b = s.iter().nth(1).unwrap().id;
        s.pin(b);
        s.remove(b);
        assert_eq!(s.pinned(), None);
        s.restore_removed();
        assert_eq!(s.pinned(), Some(b));

        // But a tab that was not pinned does not come back pinned.
        let a = s.iter().next().unwrap().id;
        s.remove(a);
        s.restore_removed();
        assert_eq!(s.pinned(), Some(b));
    }

    #[test]
    fn deleting_every_card_is_a_state_the_strip_can_be_in_and_come_back_from() {
        // D-048: emptying the strip is allowed rather than guarded against,
        // because the `[+]` slot means an empty strip still offers a way out.
        let mut s = strip_of(&["a"]);
        let a = s.iter().next().unwrap().id;
        s.remove(a);
        assert!(s.is_empty());
        assert!(s.active().is_none());

        assert!(s.restore_removed());
        assert_eq!(s.len(), 1);
        assert_eq!(s.active_index(), 0);
        assert!(s.active().is_some());
    }

    #[test]
    fn edit_mode_is_off_until_asked_for_and_toggles_back() {
        let mut s = strip_of(&["a"]);
        assert!(!s.is_editing(), "a fresh strip is for using, not changing");
        assert!(s.set_editing(true));
        assert!(s.is_editing());
        assert!(!s.set_editing(false));
        assert!(!s.is_editing());
    }

    #[test]
    fn a_tab_carries_launcher_items_and_nothing_about_windows() {
        // Model A (D-003) stated as a test: the only thing a tab holds is shortcuts.
        let mut s = strip_of(&["Praca"]);
        let id = s.iter().next().unwrap().id;
        s.get_mut(id)
            .unwrap()
            .items
            .push(LauncherItem::new("foot", "Terminal").with_icon("utilities-terminal"));
        let tab = s.get(id).unwrap();
        assert_eq!(tab.items.len(), 1);
        assert_eq!(tab.items[0].icon.as_deref(), Some("utilities-terminal"));
    }
}
