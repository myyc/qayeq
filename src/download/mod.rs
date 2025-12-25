//! Download manager for tracking and displaying downloads.

mod row;

pub use row::DownloadRow;

use gtk4::glib;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

// Thread-local shared state for pending "Save As" URIs and download tracking
// This allows TabManager (context menu), download handler, and window to communicate
thread_local! {
    static PENDING_SAVE_AS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static LAST_SAVE_DIRECTORY: RefCell<Option<PathBuf>> = RefCell::new(None);
    static DOWNLOAD_MANAGER: RefCell<GlobalDownloadManager> = RefCell::new(GlobalDownloadManager::new());
    static CANCEL_CALLBACKS: RefCell<std::collections::HashMap<u64, Box<dyn Fn()>>> = RefCell::new(std::collections::HashMap::new());
    static RESUME_CALLBACKS: RefCell<std::collections::HashMap<u64, Box<dyn Fn()>>> = RefCell::new(std::collections::HashMap::new());
}

/// Global download manager accessible via thread-local storage
struct GlobalDownloadManager {
    downloads: Vec<DownloadItem>,
    next_id: u64,
    on_changed_callbacks: Vec<Rc<dyn Fn()>>,
}

impl GlobalDownloadManager {
    fn new() -> Self {
        Self {
            downloads: Vec::new(),
            next_id: 1,
            on_changed_callbacks: Vec::new(),
        }
    }
}

/// Subscribe to download changes, returns callback ID for later removal
pub fn subscribe_to_changes<F: Fn() + 'static>(callback: F) -> usize {
    DOWNLOAD_MANAGER.with(|dm| {
        let mut dm = dm.borrow_mut();
        let id = dm.on_changed_callbacks.len();
        dm.on_changed_callbacks.push(Rc::new(callback));
        id
    })
}

/// Add a new download and return its ID
pub fn add_download(url: &str, filename: &str, destination: PathBuf) -> u64 {
    DOWNLOAD_MANAGER.with(|dm| {
        let mut dm = dm.borrow_mut();
        let id = dm.next_id;
        dm.next_id += 1;

        let item = DownloadItem {
            id,
            filename: filename.to_string(),
            url: url.to_string(),
            destination,
            total_bytes: 0,
            received_bytes: 0,
            status: DownloadStatus::InProgress,
            started_at: SystemTime::now(),
            supports_resume: false,
        };

        dm.downloads.push(item);
        log::info!("Download added: {} (id={})", filename, id);

        // Notify callbacks
        let callbacks: Vec<_> = dm.on_changed_callbacks.iter().cloned().collect();
        drop(dm);
        for callback in callbacks {
            callback();
        }

        id
    })
}

/// Update download progress
pub fn update_progress(id: u64, received: u64, total: u64) {
    DOWNLOAD_MANAGER.with(|dm| {
        let mut dm = dm.borrow_mut();
        if let Some(item) = dm.downloads.iter_mut().find(|d| d.id == id) {
            item.received_bytes = received;
            item.total_bytes = total;
        }

        // Notify callbacks
        let callbacks: Vec<_> = dm.on_changed_callbacks.iter().cloned().collect();
        drop(dm);
        for callback in callbacks {
            callback();
        }
    })
}

/// Set download status
pub fn set_download_status(id: u64, status: DownloadStatus) {
    DOWNLOAD_MANAGER.with(|dm| {
        let mut dm = dm.borrow_mut();
        if let Some(item) = dm.downloads.iter_mut().find(|d| d.id == id) {
            item.status = status.clone();
            log::info!("Download {} status: {:?}", id, status);
        }

        // Notify callbacks
        let callbacks: Vec<_> = dm.on_changed_callbacks.iter().cloned().collect();
        drop(dm);
        for callback in callbacks {
            callback();
        }
    })
}

/// Get recent downloads
pub fn recent_downloads(limit: usize) -> Vec<DownloadItem> {
    DOWNLOAD_MANAGER.with(|dm| {
        let dm = dm.borrow();
        dm.downloads.iter().rev().take(limit).cloned().collect()
    })
}

/// Check if there are any active downloads
pub fn has_active_downloads() -> bool {
    DOWNLOAD_MANAGER.with(|dm| {
        let dm = dm.borrow();
        dm.downloads.iter().any(|d| d.is_active())
    })
}

/// Check if there are any downloads
pub fn has_downloads() -> bool {
    DOWNLOAD_MANAGER.with(|dm| {
        let dm = dm.borrow();
        !dm.downloads.is_empty()
    })
}

/// Clear completed/failed downloads
pub fn clear_completed() {
    DOWNLOAD_MANAGER.with(|dm| {
        let mut dm = dm.borrow_mut();
        dm.downloads.retain(|d| d.is_active());

        // Notify callbacks
        let callbacks: Vec<_> = dm.on_changed_callbacks.iter().cloned().collect();
        drop(dm);
        for callback in callbacks {
            callback();
        }
    })
}

/// Remove a specific download by ID
pub fn remove_download(id: u64) {
    DOWNLOAD_MANAGER.with(|dm| {
        let mut dm = dm.borrow_mut();
        dm.downloads.retain(|d| d.id != id);

        // Notify callbacks
        let callbacks: Vec<_> = dm.on_changed_callbacks.iter().cloned().collect();
        drop(dm);
        for callback in callbacks {
            callback();
        }
    });

    // Also remove any cancel callback
    remove_cancel_callback(id);
}

/// Mark a URI as pending "Save As" dialog (called from context menu handler)
pub fn mark_save_as(uri: &str) {
    PENDING_SAVE_AS.with(|set| {
        set.borrow_mut().insert(uri.to_string());
    });
    log::debug!("Marked URI for Save As dialog: {}", uri);
}

/// Check if a URI should show "Save As" dialog (and remove from pending)
pub fn is_save_as(uri: &str) -> bool {
    PENDING_SAVE_AS.with(|set| set.borrow_mut().remove(uri))
}

/// Get the last directory used for "Save As", or Downloads folder
pub fn last_save_directory() -> PathBuf {
    LAST_SAVE_DIRECTORY.with(|dir| {
        dir.borrow().clone().unwrap_or_else(|| {
            glib::user_special_dir(glib::UserDirectory::Downloads)
                .unwrap_or_else(|| PathBuf::from("."))
        })
    })
}

/// Set the last directory used for "Save As"
pub fn set_last_save_directory(path: PathBuf) {
    LAST_SAVE_DIRECTORY.with(|dir| {
        *dir.borrow_mut() = Some(path);
    });
}

/// Register a cancel callback for a download
pub fn register_cancel_callback<F: Fn() + 'static>(id: u64, callback: F) {
    CANCEL_CALLBACKS.with(|callbacks| {
        callbacks.borrow_mut().insert(id, Box::new(callback));
    });
}

/// Cancel a download by ID
pub fn cancel_download(id: u64) {
    // Set status to Cancelled first (before WebKit handlers fire)
    set_download_status(id, DownloadStatus::Cancelled);
    // Then call the cancel callback
    CANCEL_CALLBACKS.with(|callbacks| {
        if let Some(callback) = callbacks.borrow().get(&id) {
            callback();
        }
    });
}

/// Remove the cancel callback for a download (called when download finishes)
pub fn remove_cancel_callback(id: u64) {
    CANCEL_CALLBACKS.with(|callbacks| {
        callbacks.borrow_mut().remove(&id);
    });
}

/// Register a resume callback for a download
pub fn register_resume_callback<F: Fn() + 'static>(id: u64, callback: F) {
    RESUME_CALLBACKS.with(|callbacks| {
        callbacks.borrow_mut().insert(id, Box::new(callback));
    });
}

/// Pause a download by ID (cancels it but marks as paused for resume)
pub fn pause_download(id: u64) {
    // Set status to Paused first (before WebKit handlers fire)
    set_download_status(id, DownloadStatus::Paused);
    // Then cancel the active download
    CANCEL_CALLBACKS.with(|callbacks| {
        if let Some(callback) = callbacks.borrow().get(&id) {
            callback();
        }
    });
}

/// Resume a paused download
pub fn resume_download(id: u64) {
    RESUME_CALLBACKS.with(|callbacks| {
        if let Some(callback) = callbacks.borrow().get(&id) {
            callback();
        }
    });
}

/// Remove the resume callback for a download
pub fn remove_resume_callback(id: u64) {
    RESUME_CALLBACKS.with(|callbacks| {
        callbacks.borrow_mut().remove(&id);
    });
}

/// Set whether a download supports resume
pub fn set_supports_resume(id: u64, supports: bool) {
    DOWNLOAD_MANAGER.with(|dm| {
        let mut dm = dm.borrow_mut();
        if let Some(item) = dm.downloads.iter_mut().find(|d| d.id == id) {
            item.supports_resume = supports;
        }
    });
}

/// Check if a download is paused
pub fn is_paused(id: u64) -> bool {
    DOWNLOAD_MANAGER.with(|dm| {
        dm.borrow()
            .downloads
            .iter()
            .find(|d| d.id == id)
            .map(|d| matches!(d.status, DownloadStatus::Paused))
            .unwrap_or(false)
    })
}

/// Check if a download is active (in progress)
pub fn is_active(id: u64) -> bool {
    DOWNLOAD_MANAGER.with(|dm| {
        dm.borrow()
            .downloads
            .iter()
            .find(|d| d.id == id)
            .map(|d| matches!(d.status, DownloadStatus::InProgress))
            .unwrap_or(false)
    })
}

/// Get paused download info for resuming
pub fn get_download_for_resume(id: u64) -> Option<(String, PathBuf, u64)> {
    DOWNLOAD_MANAGER.with(|dm| {
        dm.borrow()
            .downloads
            .iter()
            .find(|d| d.id == id && matches!(d.status, DownloadStatus::Paused))
            .map(|d| (d.url.clone(), d.destination.clone(), d.received_bytes))
    })
}

/// Status of a download
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStatus {
    InProgress,
    Paused,
    Completed,
    Failed(String),
    Cancelled,
}

/// Represents a single download
#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub id: u64,
    pub filename: String,
    pub url: String,
    pub destination: PathBuf,
    pub total_bytes: u64,
    pub received_bytes: u64,
    pub status: DownloadStatus,
    pub started_at: SystemTime,
    /// Whether the server supports range requests (resume)
    pub supports_resume: bool,
}

impl DownloadItem {
    /// Get download progress as a fraction (0.0 to 1.0)
    pub fn progress(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.received_bytes as f64 / self.total_bytes as f64
        }
    }

    /// Check if download is still in progress (actively downloading)
    pub fn is_active(&self) -> bool {
        matches!(self.status, DownloadStatus::InProgress)
    }

    /// Check if download is paused
    pub fn is_paused(&self) -> bool {
        matches!(self.status, DownloadStatus::Paused)
    }

    /// Check if download can be resumed
    pub fn can_resume(&self) -> bool {
        self.is_paused() && self.supports_resume
    }

    /// Get download speed in bytes per second
    pub fn speed_bps(&self) -> f64 {
        if !self.is_active() || self.received_bytes == 0 {
            return 0.0;
        }

        match self.started_at.elapsed() {
            Ok(elapsed) => {
                let secs = elapsed.as_secs_f64();
                if secs > 0.0 {
                    self.received_bytes as f64 / secs
                } else {
                    0.0
                }
            }
            Err(_) => 0.0,
        }
    }

    /// Get estimated time remaining in seconds
    pub fn eta_seconds(&self) -> Option<u64> {
        if !self.is_active() || self.total_bytes == 0 {
            return None;
        }

        let remaining_bytes = self.total_bytes.saturating_sub(self.received_bytes);
        if remaining_bytes == 0 {
            return Some(0);
        }

        let speed = self.speed_bps();
        if speed > 0.0 {
            Some((remaining_bytes as f64 / speed) as u64)
        } else {
            None
        }
    }

    /// Format speed as human readable string
    pub fn speed_string(&self) -> String {
        let speed = self.speed_bps();
        format_bytes_per_second(speed)
    }

    /// Format ETA as human readable string
    pub fn eta_string(&self) -> String {
        match self.eta_seconds() {
            Some(secs) if secs == 0 => "finishing...".to_string(),
            Some(secs) => format_duration(secs),
            None => "calculating...".to_string(),
        }
    }

    /// Format received/total as human readable string
    pub fn size_string(&self) -> String {
        if self.total_bytes > 0 {
            format!("{} / {}", format_bytes(self.received_bytes), format_bytes(self.total_bytes))
        } else {
            format_bytes(self.received_bytes)
        }
    }
}

/// Format bytes as human readable string (KB, MB, GB)
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format bytes per second as human readable string
fn format_bytes_per_second(bps: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bps >= GB {
        format!("{:.1} GB/s", bps / GB)
    } else if bps >= MB {
        format!("{:.1} MB/s", bps / MB)
    } else if bps >= KB {
        format!("{:.0} KB/s", bps / KB)
    } else {
        format!("{:.0} B/s", bps)
    }
}

/// Format duration in seconds as human readable string
fn format_duration(secs: u64) -> String {
    if secs >= 3600 {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        format!("{}h {}m", hours, mins)
    } else if secs >= 60 {
        let mins = secs / 60;
        let secs_rem = secs % 60;
        format!("{}m {}s", mins, secs_rem)
    } else {
        format!("{}s", secs)
    }
}
