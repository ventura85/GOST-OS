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

/// The ordered strip of tabs, plus which one is showing.
#[derive(Debug, Clone, Default)]
pub struct TabStrip {
    tabs: Vec<Tab>,
    active: usize,
    pinned: Option<TabId>,
    next_id: u32,
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
    pub fn remove(&mut self, id: TabId) -> bool {
        let Some(idx) = self.index_of(id) else {
            return false;
        };
        self.tabs.remove(idx);
        if self.pinned == Some(id) {
            self.pinned = None;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
        true
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
