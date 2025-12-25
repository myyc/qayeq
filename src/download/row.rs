//! Download row widget for the downloads popover.

use super::{DownloadItem, DownloadStatus};
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct DownloadRow {
        /// Shared download ID - updated by update(), read by callbacks
        pub download_id: Rc<Cell<u64>>,
        pub filename_label: RefCell<Option<gtk4::Label>>,
        pub status_label: RefCell<Option<gtk4::Label>>,
        pub progress_bar: RefCell<Option<gtk4::ProgressBar>>,
        pub status_icon: RefCell<Option<gtk4::Image>>,
        pub pause_button: RefCell<Option<gtk4::Button>>,
        pub action_button: RefCell<Option<gtk4::Button>>,
        pub dismiss_button: RefCell<Option<gtk4::Button>>,
        pub is_active: RefCell<Option<Rc<Cell<bool>>>>,
        pub is_paused: RefCell<Option<Rc<Cell<bool>>>>,
        pub can_resume: RefCell<Option<Rc<Cell<bool>>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DownloadRow {
        const NAME: &'static str = "QayeqDownloadRow";
        type Type = super::DownloadRow;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for DownloadRow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            obj.set_orientation(gtk4::Orientation::Vertical);
            obj.set_spacing(4);
            obj.add_css_class("download-row");

            // Top row: filename + status/action
            let top_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

            // Filename label
            let filename_label = gtk4::Label::new(None);
            filename_label.set_hexpand(true);
            filename_label.set_halign(gtk4::Align::Start);
            filename_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
            filename_label.set_max_width_chars(30);
            top_row.append(&filename_label);
            *self.filename_label.borrow_mut() = Some(filename_label);

            // Status icon (shown when completed/failed)
            let status_icon = gtk4::Image::new();
            status_icon.set_visible(false);
            top_row.append(&status_icon);
            *self.status_icon.borrow_mut() = Some(status_icon);

            // Pause/Resume button (shown when downloading or paused+resumable)
            let pause_button = gtk4::Button::new();
            pause_button.add_css_class("flat");
            pause_button.add_css_class("circular");
            pause_button.set_icon_name("media-playback-pause-symbolic");
            pause_button.set_tooltip_text(Some("Pause"));
            pause_button.set_visible(false);
            top_row.append(&pause_button);
            *self.pause_button.borrow_mut() = Some(pause_button);

            // Action button (cancel while downloading, open folder when done)
            let action_button = gtk4::Button::new();
            action_button.add_css_class("flat");
            action_button.add_css_class("circular");
            action_button.set_icon_name("process-stop-symbolic");
            action_button.set_tooltip_text(Some("Cancel"));
            top_row.append(&action_button);
            *self.action_button.borrow_mut() = Some(action_button);

            // Dismiss button (remove from list)
            let dismiss_button = gtk4::Button::new();
            dismiss_button.add_css_class("flat");
            dismiss_button.add_css_class("circular");
            dismiss_button.set_icon_name("window-close-symbolic");
            dismiss_button.set_tooltip_text(Some("Dismiss"));
            dismiss_button.set_visible(false);
            top_row.append(&dismiss_button);
            *self.dismiss_button.borrow_mut() = Some(dismiss_button);

            obj.append(&top_row);

            // Status label (speed, size, ETA)
            let status_label = gtk4::Label::new(None);
            status_label.set_halign(gtk4::Align::Start);
            status_label.add_css_class("dim-label");
            status_label.add_css_class("caption");
            obj.append(&status_label);
            *self.status_label.borrow_mut() = Some(status_label);

            // Progress bar (shown while downloading)
            let progress_bar = gtk4::ProgressBar::new();
            progress_bar.set_visible(true);
            progress_bar.add_css_class("download-progress");
            obj.append(&progress_bar);
            *self.progress_bar.borrow_mut() = Some(progress_bar);
        }
    }

    impl WidgetImpl for DownloadRow {}
    impl BoxImpl for DownloadRow {}
}

glib::wrapper! {
    pub struct DownloadRow(ObjectSubclass<imp::DownloadRow>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl DownloadRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Update the row with download item data
    pub fn update(&self, item: &DownloadItem) {
        let imp = self.imp();
        imp.download_id.set(item.id);

        // Update filename
        if let Some(label) = imp.filename_label.borrow().as_ref() {
            label.set_text(&item.filename);
        }

        // Update status label with speed, size, and ETA
        if let Some(status) = imp.status_label.borrow().as_ref() {
            match &item.status {
                DownloadStatus::InProgress => {
                    let text = format!(
                        "{} - {} left - {}",
                        item.speed_string(),
                        item.eta_string(),
                        item.size_string()
                    );
                    status.set_text(&text);
                    status.set_visible(true);
                }
                DownloadStatus::Paused => {
                    let text = if item.supports_resume {
                        format!("Paused - {}", item.size_string())
                    } else {
                        format!("Paused (resume not supported) - {}", item.size_string())
                    };
                    status.set_text(&text);
                    status.set_visible(true);
                }
                DownloadStatus::Completed => {
                    status.set_text("Completed");
                    status.set_visible(true);
                }
                DownloadStatus::Failed(err) => {
                    status.set_text(&format!("Failed: {}", err));
                    status.set_visible(true);
                }
                DownloadStatus::Cancelled => {
                    status.set_text("Cancelled");
                    status.set_visible(true);
                }
            }
        }

        // Update progress bar and visibility
        if let Some(progress) = imp.progress_bar.borrow().as_ref() {
            match &item.status {
                DownloadStatus::InProgress | DownloadStatus::Paused => {
                    progress.set_visible(true);
                    progress.set_fraction(item.progress());
                }
                _ => {
                    progress.set_visible(false);
                }
            }
        }

        // Update status icon
        if let Some(icon) = imp.status_icon.borrow().as_ref() {
            match &item.status {
                DownloadStatus::Completed => {
                    icon.set_icon_name(Some("emblem-ok-symbolic"));
                    icon.set_visible(true);
                }
                DownloadStatus::Failed(_) => {
                    icon.set_icon_name(Some("dialog-error-symbolic"));
                    icon.set_visible(true);
                }
                DownloadStatus::Cancelled => {
                    icon.set_icon_name(Some("process-stop-symbolic"));
                    icon.set_visible(true);
                }
                DownloadStatus::InProgress | DownloadStatus::Paused => {
                    icon.set_visible(false);
                }
            }
        }

        // Update pause/resume button
        if let Some(button) = imp.pause_button.borrow().as_ref() {
            match &item.status {
                DownloadStatus::InProgress => {
                    // Show pause button if server supports resume
                    button.set_icon_name("media-playback-pause-symbolic");
                    button.set_tooltip_text(Some("Pause"));
                    button.set_visible(item.supports_resume);
                }
                DownloadStatus::Paused if item.supports_resume => {
                    button.set_icon_name("media-playback-start-symbolic");
                    button.set_tooltip_text(Some("Resume"));
                    button.set_visible(true);
                }
                _ => {
                    button.set_visible(false);
                }
            }
        }

        // Update action button
        if let Some(button) = imp.action_button.borrow().as_ref() {
            match &item.status {
                DownloadStatus::InProgress => {
                    button.set_icon_name("process-stop-symbolic");
                    button.set_tooltip_text(Some("Cancel"));
                    button.set_visible(true);
                }
                DownloadStatus::Completed => {
                    button.set_icon_name("folder-open-symbolic");
                    button.set_tooltip_text(Some("Show in folder"));
                    button.set_visible(true);
                }
                _ => {
                    button.set_visible(false);
                }
            }
        }

        // Update dismiss button (shown for non-active downloads)
        if let Some(button) = imp.dismiss_button.borrow().as_ref() {
            button.set_visible(!item.is_active());
        }

        // Update state for button callbacks
        self.set_is_active(item.is_active());
        self.set_is_paused(item.is_paused());
        self.set_can_resume(item.can_resume());
    }

    /// Get the download ID this row represents
    pub fn download_id(&self) -> u64 {
        self.imp().download_id.get()
    }

    /// Connect to the action button click
    /// Callback receives (download_id, is_cancel) where is_cancel is true if the download is active
    pub fn connect_action_clicked<F: Fn(u64, bool) + 'static>(&self, f: F) {
        if let Some(button) = self.imp().action_button.borrow().as_ref() {
            let imp = self.imp();
            let download_id = Rc::clone(&imp.download_id);
            let is_active = Rc::new(Cell::new(true)); // Track if download is active

            // Store is_active state so we can update it
            *imp.is_active.borrow_mut() = Some(Rc::clone(&is_active));

            let f = Rc::new(f);
            button.connect_clicked(move |_| {
                let id = download_id.get();
                let cancel = is_active.get();
                f(id, cancel);
            });
        }
    }

    /// Update the active state for the action button callback
    pub fn set_is_active(&self, active: bool) {
        if let Some(is_active) = self.imp().is_active.borrow().as_ref() {
            is_active.set(active);
        }
    }

    /// Update the paused state
    pub fn set_is_paused(&self, paused: bool) {
        if let Some(is_paused) = self.imp().is_paused.borrow().as_ref() {
            is_paused.set(paused);
        }
    }

    /// Update the can_resume state
    pub fn set_can_resume(&self, can_resume: bool) {
        if let Some(cr) = self.imp().can_resume.borrow().as_ref() {
            cr.set(can_resume);
        }
    }

    /// Connect to the dismiss button click
    pub fn connect_dismiss_clicked<F: Fn(u64) + 'static>(&self, f: F) {
        if let Some(button) = self.imp().dismiss_button.borrow().as_ref() {
            let download_id = Rc::clone(&self.imp().download_id);
            button.connect_clicked(move |_| {
                f(download_id.get());
            });
        }
    }

    /// Connect to the pause/resume button click
    /// Callback receives (download_id, is_paused) - if is_paused, resume; otherwise pause
    pub fn connect_pause_clicked<F: Fn(u64, bool) + 'static>(&self, f: F) {
        if let Some(button) = self.imp().pause_button.borrow().as_ref() {
            let imp = self.imp();
            let download_id = Rc::clone(&imp.download_id);
            let is_paused = Rc::new(Cell::new(false));

            // Store is_paused state so we can update it
            *imp.is_paused.borrow_mut() = Some(Rc::clone(&is_paused));

            button.connect_clicked(move |_| {
                f(download_id.get(), is_paused.get());
            });
        }
    }
}

impl Default for DownloadRow {
    fn default() -> Self {
        Self::new()
    }
}
