use crate::config;
use crate::history::HistoryDatabase;
use gtk4::glib;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use webkit6::prelude::*;
use webkit6::{CookieAcceptPolicy, CookiePersistentStorage, NetworkSession};

/// Error type for profile operations
#[derive(Debug)]
pub enum ProfileError {
    Io(std::io::Error),
    Database(rusqlite::Error),
    NotFound(String),
    AlreadyExists(String),
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileError::Io(e) => write!(f, "IO error: {}", e),
            ProfileError::Database(e) => write!(f, "Database error: {}", e),
            ProfileError::NotFound(name) => write!(f, "Profile not found: {}", name),
            ProfileError::AlreadyExists(name) => write!(f, "Profile already exists: {}", name),
        }
    }
}

impl std::error::Error for ProfileError {}

impl From<std::io::Error> for ProfileError {
    fn from(e: std::io::Error) -> Self {
        ProfileError::Io(e)
    }
}

impl From<rusqlite::Error> for ProfileError {
    fn from(e: rusqlite::Error) -> Self {
        ProfileError::Database(e)
    }
}

/// Represents a browser profile with its own data directory
pub struct Profile {
    name: String,
    path: PathBuf,
    history_db: HistoryDatabase,
    network_session: NetworkSession,
}

impl std::fmt::Debug for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Profile")
            .field("name", &self.name)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Profile {
    /// Create or open a profile at the given path
    pub fn open(name: &str, path: PathBuf) -> Result<Self, ProfileError> {
        // Ensure directory exists
        fs::create_dir_all(&path)?;

        // Create data and cache subdirectories for WebKit
        let data_dir = path.join("data");
        let cache_dir = path.join("cache");
        fs::create_dir_all(&data_dir)?;
        fs::create_dir_all(&cache_dir)?;

        // Open history database
        let history_db = HistoryDatabase::new(&path)?;

        // Create NetworkSession with profile-specific paths for cookie/storage persistence
        let network_session = NetworkSession::new(
            data_dir.to_str(),
            cache_dir.to_str(),
        );

        // Configure CookieManager for persistent storage
        if let Some(cookie_manager) = network_session.cookie_manager() {
            // Accept first-party cookies but block third-party (good privacy default)
            cookie_manager.set_accept_policy(CookieAcceptPolicy::NoThirdParty);

            // Set persistent storage path for cookies
            let cookies_path = data_dir.join("cookies.sqlite");
            cookie_manager.set_persistent_storage(
                cookies_path.to_str().expect("valid UTF-8 path"),
                CookiePersistentStorage::Sqlite,
            );

            log::info!(
                "Profile '{}' cookie manager configured: cookies stored at {:?}",
                name,
                cookies_path
            );
        } else {
            log::warn!("Profile '{}': could not get cookie manager from network session", name);
        }

        // Enable favicon loading
        if let Some(data_manager) = network_session.website_data_manager() {
            data_manager.set_favicons_enabled(true);
            log::info!("Profile '{}': favicons enabled", name);
        }

        // Set up download handling
        Self::setup_download_handler(&network_session);

        log::info!(
            "Profile '{}' network session created with data={:?}, cache={:?}",
            name,
            data_dir,
            cache_dir
        );

        Ok(Self {
            name: name.to_string(),
            path,
            history_db,
            network_session,
        })
    }

    /// Get the profile name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the profile directory path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get access to the history database
    pub fn history(&self) -> &HistoryDatabase {
        &self.history_db
    }

    /// Get mutable access to the history database
    pub fn history_mut(&mut self) -> &mut HistoryDatabase {
        &mut self.history_db
    }

    /// Get the network session for WebKit
    pub fn network_session(&self) -> &NetworkSession {
        &self.network_session
    }

    /// Set up download handling for the network session
    fn setup_download_handler(network_session: &NetworkSession) {
        use crate::download;
        use std::cell::Cell;
        use std::rc::Rc;

        network_session.connect_download_started(|_session, wk_download| {
            let uri = wk_download.request().and_then(|r| r.uri()).map(|u| u.to_string());
            log::info!("Download started: {:?}", uri);


            // Check if this download was triggered by context menu "Save As"
            let show_dialog = uri.as_ref().map(|u| download::is_save_as(u)).unwrap_or(false);

            // Track download ID across closures
            let download_id: Rc<Cell<Option<u64>>> = Rc::new(Cell::new(None));
            let download_id_for_decide = Rc::clone(&download_id);
            let download_id_for_progress = Rc::clone(&download_id);
            let download_id_for_finish = Rc::clone(&download_id);
            let download_id_for_fail = Rc::clone(&download_id);

            let uri_for_decide = uri.clone();

            // Connect to decide-destination
            wk_download.connect_decide_destination(move |wk_download, suggested_filename| {
                log::info!("Download decide destination: {} (dialog={})", suggested_filename, show_dialog);

                if show_dialog {
                    // Show file chooser dialog (uses XDG portals)
                    let initial_dir = download::last_save_directory();

                    let dialog = gtk4::FileDialog::builder()
                        .title("Save As")
                        .initial_name(suggested_filename)
                        .initial_folder(&gtk4::gio::File::for_path(&initial_dir))
                        .build();

                    let download_clone = wk_download.clone();
                    let download_for_cancel = wk_download.clone();
                    let download_id_clone = Rc::clone(&download_id_for_decide);
                    let uri_clone = uri_for_decide.clone();
                    let filename = suggested_filename.to_string();

                    let app = gtk4::gio::Application::default()
                        .and_then(|a| a.downcast::<gtk4::Application>().ok());
                    let window = app.as_ref().and_then(|a| a.active_window());

                    dialog.save(
                        window.as_ref(),
                        gtk4::gio::Cancellable::NONE,
                        move |result| {
                            match result {
                                Ok(file) => {
                                    if let Some(path) = file.path() {
                                        // Remember the directory for next time
                                        if let Some(parent) = path.parent() {
                                            download::set_last_save_directory(parent.to_path_buf());
                                        }

                                        // Register with global download manager
                                        let id = download::add_download(
                                            uri_clone.as_deref().unwrap_or(""),
                                            &filename,
                                            path.clone(),
                                        );
                                        download_id_clone.set(Some(id));

                                        // Register cancel callback
                                        let download_for_cancel = download_for_cancel.clone();
                                        download::register_cancel_callback(id, move || {
                                            download_for_cancel.cancel();
                                        });

                                        let path_str = path.to_string_lossy().to_string();
                                        log::info!("Saving download to: {}", path_str);
                                        download_clone.set_allow_overwrite(true);
                                        download_clone.set_destination(&path_str);
                                    } else {
                                        log::error!("Could not get path from selected file");
                                        download_clone.cancel();
                                    }
                                }
                                Err(e) => {
                                    log::info!("Download save cancelled: {}", e);
                                    download_clone.cancel();
                                }
                            }
                        },
                    );

                    true // We're handling it - download waits until we set destination
                } else {
                    // Auto-save to Downloads folder
                    let downloads_dir = glib::user_special_dir(glib::UserDirectory::Downloads)
                        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())));

                    // Generate unique filename if file already exists
                    let destination = unique_filename(&downloads_dir, suggested_filename);
                    let final_filename = destination
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| suggested_filename.to_string());

                    // Register with global download manager
                    let id = download::add_download(
                        uri_for_decide.as_deref().unwrap_or(""),
                        &final_filename,
                        destination.clone(),
                    );
                    download_id_for_decide.set(Some(id));

                    // Register cancel callback - preserves file if pausing
                    let download_for_cancel = wk_download.clone();
                    let dest_for_cancel = destination.clone();
                    download::register_cancel_callback(id, move || {
                        // If we're pausing (not cancelling), backup the partial file
                        // because WebKit will delete it on cancel
                        if download::is_paused(id) {
                            let backup_path = dest_for_cancel.with_extension("part");
                            if let Err(e) = std::fs::copy(&dest_for_cancel, &backup_path) {
                                log::warn!("Failed to backup partial download: {}", e);
                            } else {
                                log::info!("Backed up partial download to {:?}", backup_path);
                            }
                        }
                        download_for_cancel.cancel();
                    });

                    // Register resume callback with Range-based HTTP resume
                    download::register_resume_callback(id, move || {
                        if let Some((url, dest, received_bytes)) = download::get_download_for_resume(id) {
                            log::info!("Resuming download {} from byte {}", id, received_bytes);
                            resume_download_with_range(id, &url, &dest, received_bytes);
                        }
                    });

                    let path_str = destination.to_string_lossy().to_string();
                    log::info!("Auto-saving download to: {}", path_str);
                    // Allow overwrite since unique_filename already guarantees a unique path
                    wk_download.set_allow_overwrite(true);
                    wk_download.set_destination(&path_str);

                    true // We've set the destination
                }
            });

            // Track if we've checked for resume support
            let resume_checked: Rc<Cell<bool>> = Rc::new(Cell::new(false));

            // Update progress
            wk_download.connect_received_data(move |wk_download, _data_len| {
                if let Some(id) = download_id_for_progress.get() {
                    if let Some(response) = wk_download.response() {
                        let total = response.content_length() as u64;
                        let received = wk_download.received_data_length();
                        download::update_progress(id, received, total);

                        // Check Accept-Ranges header on first data received
                        if !resume_checked.get() {
                            resume_checked.set(true);
                            if let Some(headers) = response.http_headers() {
                                if let Some(accept_ranges) = headers.one("Accept-Ranges") {
                                    let supports_resume = accept_ranges.to_lowercase() != "none";
                                    download::set_supports_resume(id, supports_resume);
                                    log::info!("Download {} Accept-Ranges: {} (resume={})",
                                        id, accept_ranges, supports_resume);
                                }
                            }
                        }
                    }
                }
            });

            // Handle completion - only if still in progress (not paused/cancelled)
            wk_download.connect_finished(move |wk_download| {
                if let Some(id) = download_id_for_finish.get() {
                    // Only mark as completed if we weren't paused/cancelled
                    // Check if download actually completed (no error, or received all data)
                    let was_cancelled = wk_download.response()
                        .map(|r| {
                            let expected = r.content_length() as u64;
                            let received = wk_download.received_data_length();
                            expected > 0 && received < expected
                        })
                        .unwrap_or(false);

                    if !was_cancelled && !download::is_paused(id) {
                        download::set_download_status(id, download::DownloadStatus::Completed);
                        download::remove_cancel_callback(id);
                        download::remove_resume_callback(id);
                    }
                }
            });

            // Handle failures - but not if we're pausing (that also triggers failed)
            wk_download.connect_failed(move |_wk_download, error| {
                if let Some(id) = download_id_for_fail.get() {
                    // Check if this was a pause or cancel (status already set)
                    if !download::is_paused(id) && download::is_active(id) {
                        download::set_download_status(
                            id,
                            download::DownloadStatus::Failed(error.to_string()),
                        );
                    }
                    download::remove_cancel_callback(id);
                    // Don't remove resume callback - we might want to resume
                }
            });
        });

        log::info!("Download handler configured for network session");
    }
}

/// Manages browser profiles (containers)
/// Supports multiple simultaneously loaded profiles for container isolation
#[derive(Debug)]
pub struct ProfileManager {
    base_dir: PathBuf,
    profiles_dir: PathBuf,
    /// Currently loaded profiles (lazy-loaded on demand)
    profiles: HashMap<String, Profile>,
    /// Default container for new tabs
    default_container: String,
}

impl ProfileManager {
    /// Create a new profile manager, initializing the data directories
    pub fn new() -> Result<Self, ProfileError> {
        // Get XDG data directory
        let base_dir = directories::ProjectDirs::from("dev", "myyc", "qayeq")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| {
                // Fallback to ~/.local/share/qayeq
                dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("~/.local/share"))
                    .join("qayeq")
            });

        let profiles_dir = base_dir.join(config::PROFILES_DIR);

        // Create directories if they don't exist
        fs::create_dir_all(&profiles_dir)?;

        let mut manager = Self {
            base_dir,
            profiles_dir,
            profiles: HashMap::new(),
            default_container: config::DEFAULT_CONTAINER.to_string(),
        };

        // Ensure all default containers exist
        for (container_id, container_name, _color) in config::DEFAULT_CONTAINERS {
            if !manager.profile_exists(container_id) {
                log::info!("Creating container profile '{}' ({})", container_name, container_id);
                manager.create_profile(container_id)?;
            }
        }

        // Pre-load the default container
        manager.load_profile(config::DEFAULT_CONTAINER)?;

        Ok(manager)
    }

    /// Check if a profile/container exists on disk
    pub fn profile_exists(&self, name: &str) -> bool {
        self.profiles_dir.join(name).exists()
    }

    /// Create a new profile directory (does not load it)
    pub fn create_profile(&mut self, name: &str) -> Result<(), ProfileError> {
        let profile_path = self.profiles_dir.join(name);

        if profile_path.exists() {
            return Err(ProfileError::AlreadyExists(name.to_string()));
        }

        // Create profile directory
        fs::create_dir_all(&profile_path)?;

        log::info!("Created profile '{}' at {:?}", name, profile_path);

        Ok(())
    }

    /// Load a profile into memory (if not already loaded)
    fn load_profile(&mut self, name: &str) -> Result<(), ProfileError> {
        if self.profiles.contains_key(name) {
            return Ok(()); // Already loaded
        }

        let profile_path = self.profiles_dir.join(name);
        if !profile_path.exists() {
            return Err(ProfileError::NotFound(name.to_string()));
        }

        let profile = Profile::open(name, profile_path)?;
        self.profiles.insert(name.to_string(), profile);
        log::info!("Loaded profile/container '{}'", name);

        Ok(())
    }

    /// Get a profile by name, loading it if necessary
    pub fn get_or_load_profile(&mut self, name: &str) -> Result<&Profile, ProfileError> {
        self.load_profile(name)?;
        self.profiles.get(name).ok_or_else(|| ProfileError::NotFound(name.to_string()))
    }

    /// Get a loaded profile by name (without loading)
    pub fn get_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    /// Get mutable access to a profile
    pub fn get_profile_mut(&mut self, name: &str) -> Option<&mut Profile> {
        self.profiles.get_mut(name)
    }

    /// List all available profiles on disk
    pub fn list_profiles(&self) -> Result<Vec<String>, ProfileError> {
        let mut profiles = Vec::new();

        for entry in fs::read_dir(&self.profiles_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    profiles.push(name.to_string());
                }
            }
        }

        profiles.sort();
        Ok(profiles)
    }

    /// List currently loaded profiles
    pub fn loaded_profiles(&self) -> Vec<&str> {
        self.profiles.keys().map(|s| s.as_str()).collect()
    }

    /// Get the default container name
    pub fn default_container(&self) -> &str {
        &self.default_container
    }

    /// Set the default container for new tabs
    pub fn set_default_container(&mut self, name: &str) -> Result<(), ProfileError> {
        // Ensure the container exists and is loaded
        self.load_profile(name)?;
        self.default_container = name.to_string();
        log::info!("Default container set to '{}'", name);
        Ok(())
    }

    /// Get the current/default profile (backwards compatibility)
    pub fn current_profile(&self) -> Option<&Profile> {
        self.profiles.get(&self.default_container)
    }

    /// Get mutable access to the current/default profile (backwards compatibility)
    pub fn current_profile_mut(&mut self) -> Option<&mut Profile> {
        self.profiles.get_mut(&self.default_container)
    }

    /// Delete a profile (cannot delete loaded profiles or default containers)
    pub fn delete_profile(&mut self, name: &str) -> Result<(), ProfileError> {
        // Don't allow deleting any of the default containers
        if config::DEFAULT_CONTAINERS.iter().any(|(id, _, _)| *id == name) {
            return Err(ProfileError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Cannot delete a default container",
            )));
        }

        // Don't allow deleting currently loaded profiles (they may have open tabs)
        if self.profiles.contains_key(name) {
            return Err(ProfileError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Cannot delete a loaded profile (close all tabs using it first)",
            )));
        }

        let profile_path = self.profiles_dir.join(name);

        if !profile_path.exists() {
            return Err(ProfileError::NotFound(name.to_string()));
        }

        fs::remove_dir_all(&profile_path)?;
        log::info!("Deleted profile '{}'", name);

        Ok(())
    }

    /// Get the base data directory
    pub fn base_dir(&self) -> &PathBuf {
        &self.base_dir
    }

    /// Get the profiles directory
    pub fn profiles_dir(&self) -> &PathBuf {
        &self.profiles_dir
    }
}

// Convenience function for getting the data directory
fn dirs_data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
}

mod dirs {
    use std::path::PathBuf;

    pub fn data_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
    }
}

/// Resume a download using HTTP Range request
fn resume_download_with_range(id: u64, url: &str, dest: &PathBuf, start_byte: u64) {
    use crate::download;
    use soup::gio::prelude::CancellableExt;
    use soup::prelude::*;
    use std::fs::OpenOptions;
    use std::io::Write;

    let url = url.to_string();
    let dest = dest.clone();

    // Restore backup file if it exists (WebKit deletes the original on cancel)
    let backup_path = dest.with_extension("part");
    if backup_path.exists() && !dest.exists() {
        if let Err(e) = std::fs::rename(&backup_path, &dest) {
            log::error!("Failed to restore backup file: {}", e);
            download::set_download_status(id, download::DownloadStatus::Failed(format!("Failed to restore backup: {}", e)));
            return;
        }
        log::info!("Restored backup file from {:?}", backup_path);
    }

    // Create cancellable for this download
    let cancellable = soup::gio::Cancellable::new();
    let cancellable_for_callback = cancellable.clone();
    let dest_for_callback = dest.clone();

    // Register cancel callback for pausing/cancelling the soup-based download
    download::register_cancel_callback(id, move || {
        log::info!("Cancelling soup download {}", id);
        cancellable_for_callback.cancel();
        // Backup file if pausing
        if download::is_paused(id) {
            let backup_path = dest_for_callback.with_extension("part");
            if let Err(e) = std::fs::copy(&dest_for_callback, &backup_path) {
                log::warn!("Failed to backup partial download: {}", e);
            } else {
                log::info!("Backed up partial download to {:?}", backup_path);
            }
        }
    });

    // Set status to InProgress
    download::set_download_status(id, download::DownloadStatus::InProgress);

    glib::spawn_future_local(async move {
        let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
            // Create HTTP request with Range header
            let msg = soup::Message::new("GET", &url)?;
            msg.request_headers()
                .ok_or("No request headers")?
                .append("Range", &format!("bytes={}-", start_byte));

            let session = soup::Session::new();

            // Send request and get input stream (with cancellable)
            let input_stream = session
                .send_future(&msg, soup::glib::Priority::DEFAULT)
                .await?;

            // Check response status
            let status = msg.status();
            if status != soup::Status::PartialContent && status != soup::Status::Ok {
                return Err(format!("Server returned status {:?}", status).into());
            }

            // Get content length from response
            let content_length = msg.response_headers()
                .map(|h: soup::MessageHeaders| h.content_length())
                .unwrap_or(0);
            let total_bytes = if status == soup::Status::PartialContent {
                // For 206, content-length is the remaining bytes
                start_byte + content_length as u64
            } else {
                // For 200, server doesn't support Range - we're getting the whole file
                content_length as u64
            };

            // Open destination file for appending (or writing if server sent full file)
            let mut file = if status == soup::Status::PartialContent {
                OpenOptions::new().append(true).open(&dest)?
            } else {
                // Server didn't support Range, start from beginning
                download::update_progress(id, 0, total_bytes);
                OpenOptions::new().write(true).truncate(true).create(true).open(&dest)?
            };

            // Read and write in chunks
            let mut received = if status == soup::Status::PartialContent { start_byte } else { 0 };
            let mut buffer = vec![0u8; 65536]; // 64KB chunks

            loop {
                // Check for cancellation before each read
                if cancellable.is_cancelled() {
                    log::info!("Download {} cancelled during read loop", id);
                    return Ok(()); // Exit cleanly, status already set by cancel callback
                }

                // read_future returns (buffer, bytes_read) or (buffer, error)
                let result = input_stream
                    .read_future(buffer, soup::glib::Priority::DEFAULT)
                    .await;

                let (buf, bytes_read) = match result {
                    Ok((b, n)) => (b, n),
                    Err((_b, e)) => {
                        // Check if this was due to cancellation
                        if cancellable.is_cancelled() {
                            log::info!("Download {} cancelled", id);
                            return Ok(());
                        }
                        return Err(format!("Read error: {}", e).into());
                    }
                };
                buffer = buf; // Take buffer back

                if bytes_read == 0 {
                    break; // EOF
                }

                file.write_all(&buffer[..bytes_read])?;
                received += bytes_read as u64;

                // Update progress
                download::update_progress(id, received, total_bytes);
            }

            Ok(())
        }.await;

        // Only update status if not already paused/cancelled
        if !download::is_paused(id) && download::is_active(id) {
            match result {
                Ok(()) => {
                    log::info!("Download {} resumed and completed", id);
                    download::set_download_status(id, download::DownloadStatus::Completed);
                    download::remove_cancel_callback(id);
                    download::remove_resume_callback(id);
                }
                Err(e) => {
                    log::error!("Resume download {} failed: {}", id, e);
                    download::set_download_status(id, download::DownloadStatus::Failed(e.to_string()));
                }
            }
        }
    });
}

/// Generate a unique filename by adding a counter if the file already exists.
/// Example: "file.txt" -> "file.(1).txt" -> "file.(2).txt"
fn unique_filename(dir: &PathBuf, filename: &str) -> PathBuf {
    let base_path = dir.join(filename);

    if !base_path.exists() {
        return base_path;
    }

    // Split filename into stem and extension
    let (stem, ext) = if let Some(dot_pos) = filename.rfind('.') {
        (&filename[..dot_pos], Some(&filename[dot_pos..]))
    } else {
        (filename, None)
    };

    // Try incrementing counter until we find a free filename
    for counter in 1..1000 {
        let new_filename = match ext {
            Some(ext) => format!("{}.({}){}",  stem, counter, ext),
            None => format!("{}.({})", stem, counter),
        };
        let new_path = dir.join(&new_filename);
        if !new_path.exists() {
            return new_path;
        }
    }

    // Fallback: just use a timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let new_filename = match ext {
        Some(ext) => format!("{}.{}{}",  stem, timestamp, ext),
        None => format!("{}.{}", stem, timestamp),
    };
    dir.join(&new_filename)
}
