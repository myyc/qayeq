//! Session persistence for saving and restoring browser tabs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

const SESSION_FILE: &str = "session.json";

/// A saved tab entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedTab {
    pub url: String,
    pub title: Option<String>,
    #[serde(default)]
    pub container: Option<String>,
}

/// A saved window with its tabs
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedWindow {
    pub tabs: Vec<SavedTab>,
    pub active_tab_index: usize,
    #[serde(default)]
    pub window_width: Option<i32>,
    #[serde(default)]
    pub window_height: Option<i32>,
}

/// Multi-window session state to persist across restarts
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MultiWindowSession {
    pub windows: Vec<SavedWindow>,
}

impl MultiWindowSession {
    /// Create an empty multi-window session
    pub fn new() -> Self {
        Self::default()
    }

    /// Load multi-window session from a profile directory.
    /// Handles migration from legacy single-window format.
    pub fn load(profile_path: &Path) -> io::Result<Self> {
        let session_path = profile_path.join(SESSION_FILE);

        if !session_path.exists() {
            return Ok(Self::new());
        }

        let contents = fs::read_to_string(&session_path)?;

        // Try to parse as multi-window format first
        if let Ok(multi) = serde_json::from_str::<MultiWindowSession>(&contents) {
            if !multi.windows.is_empty() {
                return Ok(multi);
            }
        }

        // Fall back to legacy single-window format
        if let Ok(legacy) = serde_json::from_str::<Session>(&contents) {
            if !legacy.tabs.is_empty() {
                let window = SavedWindow {
                    tabs: legacy.tabs,
                    active_tab_index: legacy.active_tab_index,
                    window_width: legacy.window_width,
                    window_height: legacy.window_height,
                };
                return Ok(MultiWindowSession {
                    windows: vec![window],
                });
            }
        }

        Ok(Self::new())
    }

    /// Save multi-window session to a profile directory
    pub fn save(&self, profile_path: &Path) -> io::Result<()> {
        let session_path = profile_path.join(SESSION_FILE);
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(&session_path, contents)?;
        log::debug!("Multi-window session saved to {:?}", session_path);
        Ok(())
    }

    /// Check if there are any windows to restore
    pub fn has_windows(&self) -> bool {
        self.windows.iter().any(|w| !w.tabs.is_empty())
    }

    /// Add a window to the session
    pub fn add_window(&mut self, window: SavedWindow) {
        self.windows.push(window);
    }
}

/// Session state to persist across restarts (legacy single-window format)
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Session {
    pub tabs: Vec<SavedTab>,
    pub active_tab_index: usize,
    #[serde(default)]
    pub window_width: Option<i32>,
    #[serde(default)]
    pub window_height: Option<i32>,
}

impl Session {
    /// Create an empty session
    pub fn new() -> Self {
        Self::default()
    }

    /// Load session from a profile directory
    pub fn load(profile_path: &Path) -> io::Result<Self> {
        let session_path = profile_path.join(SESSION_FILE);

        if !session_path.exists() {
            return Ok(Self::new());
        }

        let contents = fs::read_to_string(&session_path)?;
        serde_json::from_str(&contents).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, e)
        })
    }

    /// Save session to a profile directory
    pub fn save(&self, profile_path: &Path) -> io::Result<()> {
        let session_path = profile_path.join(SESSION_FILE);
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(&session_path, contents)?;
        log::debug!("Session saved to {:?}", session_path);
        Ok(())
    }

    /// Check if there are any tabs to restore
    pub fn has_tabs(&self) -> bool {
        !self.tabs.is_empty()
    }

    /// Add a tab to the session
    pub fn add_tab(&mut self, url: String, title: Option<String>) {
        self.tabs.push(SavedTab { url, title, container: None });
    }

    /// Clear all tabs
    pub fn clear(&mut self) {
        self.tabs.clear();
        self.active_tab_index = 0;
    }
}
