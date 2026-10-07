//! KeyQuest desktop app (GTK 4 + libadwaita).

mod colors;
mod hands;
mod keyboard;
mod practice;
mod results;
mod settings;
mod state;
mod stats;
mod window;

use adw::prelude::*;
use gtk::{gdk, glib};

pub const APP_ID: &str = "io.github.efe_clk.KeyQuest";

const CSS: &str = "
.practice-card { padding: 18px 22px; }
.practice-text { line-height: 1.5; }
";

/// Logs to stderr; `RUST_LOG` picks the level (default: warnings only).
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

fn main() -> glib::ExitCode {
    init_logging();
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        gtk::Window::set_default_icon_name(APP_ID);
        let css = gtk::CssProvider::new();
        css.load_from_string(CSS);
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
    app.connect_activate(|app| {
        // A second launch focuses the existing window.
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        let win = window::Window::new(app);
        win.window.present();
        // Keep the controller alive as long as the window exists.
        let keep = std::cell::RefCell::new(Some(win.clone()));
        win.window.connect_destroy(move |_| {
            keep.borrow_mut().take();
        });
    });
    app.run()
}
