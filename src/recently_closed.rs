//! Recently closed tabs and windows tracking.

use std::collections::VecDeque;
use std::time::SystemTime;

/// Maximum number of recently closed tabs to remember per window.
pub const MAX_CLOSED_TABS: usize = 25;

/// Maximum number of recently closed windows to remember globally.
pub const MAX_CLOSED_WINDOWS: usize = 10;

/// A tab that was closed and can be reopened.
#[derive(Clone, Debug)]
pub struct ClosedTab {
    pub url: String,
    pub title: Option<String>,
    pub container: String,
    pub closed_at: SystemTime,
}

/// A window that was closed and can be reopened.
#[derive(Clone, Debug)]
pub struct ClosedWindow {
    pub tabs: Vec<ClosedTab>,
    pub active_tab_index: usize,
    pub closed_at: SystemTime,
}

/// Tracks recently closed tabs for a single window.
#[derive(Default)]
pub struct RecentlyClosedTabs {
    tabs: VecDeque<ClosedTab>,
}

impl RecentlyClosedTabs {
    pub fn new() -> Self {
        Self {
            tabs: VecDeque::new(),
        }
    }

    /// Record a closed tab. Oldest tabs are removed when limit is reached.
    pub fn push(&mut self, tab: ClosedTab) {
        self.tabs.push_front(tab);
        while self.tabs.len() > MAX_CLOSED_TABS {
            self.tabs.pop_back();
        }
    }

    /// Get and remove the most recently closed tab.
    pub fn pop(&mut self) -> Option<ClosedTab> {
        self.tabs.pop_front()
    }

    /// Get all recently closed tabs (most recent first).
    pub fn list(&self) -> impl Iterator<Item = &ClosedTab> {
        self.tabs.iter()
    }

    /// Check if there are any closed tabs.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
}

/// Tracks recently closed windows globally (across the application).
#[derive(Default)]
pub struct RecentlyClosedWindows {
    windows: VecDeque<ClosedWindow>,
}

impl RecentlyClosedWindows {
    pub fn new() -> Self {
        Self {
            windows: VecDeque::new(),
        }
    }

    /// Record a closed window. Oldest windows are removed when limit is reached.
    pub fn push(&mut self, window: ClosedWindow) {
        self.windows.push_front(window);
        while self.windows.len() > MAX_CLOSED_WINDOWS {
            self.windows.pop_back();
        }
    }

    /// Get and remove the most recently closed window.
    pub fn pop(&mut self) -> Option<ClosedWindow> {
        self.windows.pop_front()
    }

    /// Get all recently closed windows (most recent first).
    pub fn list(&self) -> impl Iterator<Item = &ClosedWindow> {
        self.windows.iter()
    }

    /// Check if there are any closed windows.
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }
}
