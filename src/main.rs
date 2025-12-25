mod application;
mod clear_data;
mod completion;
mod config;
mod content_filter;
mod download;
mod history;
mod preferences;
mod profile;
mod recently_closed;
mod session;
mod tab;
mod url_bar;
mod webview;
mod window;

use gtk4::gio;
use gtk4::prelude::*;

fn main() -> glib::ExitCode {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Starting {} v{}", config::APP_NAME, config::APP_VERSION);

    // Register compiled GResources (icons, CSS, filter lists, etc.)
    gio::resources_register_include!("qayeq.gresource")
        .expect("Failed to register resources");

    // Initialize GTK and libadwaita
    let app = application::QayeqApplication::new();
    app.run()
}
