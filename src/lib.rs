//! Unfocol is a minimalist focus timer: a colored window that shifts through
//! a theme's gradient as a focus session progresses, so the passage of time
//! is visible at a glance without reading a clock.
//!
//! This crate is the library half of the app — theme and settings loading
//! ([`load_themes`], [`load_settings`]), the color/theme model
//! ([`Color`], [`Theme`]), the countdown [`Timer`], and the [`Unfocol`]
//! `eframe::App` itself — while `main.rs` just wires it up and runs it.
mod app;
mod clock;
mod color;
mod config;
mod message;
mod notifications;
mod settings;
mod timer;

pub use app::{Unfocol, OmarchyWatcher};
pub use color::{Color, Stop, Theme};
pub use config::{
    Config, DEFAULT_THEME, DEFAULT_THEMES, Settings, SettingsLoadingOutcome, load_settings,
    load_themes, sanitize_selected_theme, get_or_create_config_dir,ensure_themes_toml_exists, omarchy_colors_toml, omarchy_theme, omarchy_current
};
pub use message::{Message, Severity};
pub use settings::write_atomic;
pub use timer::Timer;
