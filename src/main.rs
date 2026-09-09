use eframe::{Renderer, egui};
use log::{info, warn};
use unfocol::{
    Config, DEFAULT_THEME, Message, SettingsLoadingOutcome, Unfocol, OmarchyWatcher, get_or_create_config_dir, load_settings, ensure_themes_toml_exists,
    load_themes, sanitize_selected_theme, write_atomic, omarchy_current,
};

fn main() -> eframe::Result {
    env_logger::init();

    info!("Setting up configuration paths");
    let config_dir = get_or_create_config_dir();
    let themes_toml = config_dir.join("themes.toml");
    ensure_themes_toml_exists(&themes_toml).expect("Could not create themes.toml");
    let settings_toml = config_dir.join("settings.toml");

    info!("Loading themes from {}", themes_toml.display());
    let (themes, load_theme_errors) = load_themes(themes_toml);
    info!("Loading settings from {}", settings_toml.display());
    let (mut settings, settings_loading_outcome) = load_settings(&settings_toml);

    if matches!(settings_loading_outcome, SettingsLoadingOutcome::FirstRun) {
        settings.show_welcome_message = true;
        info!("Writing settings.toml");
        let toml_str =
            toml::to_string(&settings).expect("Default settings should always serialize to TOML");
        if let Err(e) = write_atomic(&toml_str, &settings_toml) {
            warn!("Failed to write settings.toml to disk: {e:?}");
        }
    }

    let mut messages: Vec<Message> = load_theme_errors
        .into_iter()
        .map(Message::from_load_themes_error)
        .collect();
    messages.extend(Message::from_settings_loading_outcome(
        settings_loading_outcome,
    ));

    if settings.show_welcome_message {
        messages.push(Message::welcome_message());
    }

    debug_assert!(themes.contains_key(DEFAULT_THEME));
    let (settings, correction) = sanitize_selected_theme(settings, &themes);
    if let Some(correction) = correction {
        let message = Message::from_settings_correction(correction);
        messages.push(message);
    }

    let config = Config::new(themes, settings);

    let renderer = {
        #[cfg(target_os = "linux")]
        {
            info!("Linux detected, choosing Glow as renderer");
            Renderer::Glow
        }
        #[cfg(not(target_os = "linux"))]
        {
            info!("Choosing Wgpu as renderer");
            Renderer::Wgpu
        }
    };

    info!("Starting app");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("se.johnkinell.Unfocol")
            .with_inner_size([100.0, 1080.0])
            .with_min_inner_size([10.0, 10.0])
            .with_position([1820.0, 0.0])
            .with_decorations(false)
            .with_resizable(true)
            .with_movable_by_background(true),
        persist_window: true,
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "Unfocol",
        options,
        Box::new(|cc| {
            let watcher = omarchy_current().and_then(|path| {
                match OmarchyWatcher::new(&path, cc.egui_ctx.clone()) {
                    Ok(w) => Some(w),
                    Err(e) if is_not_found(&e) => None,   // no Omarchy — expected
                    Err(e) => {
                        messages.push(Message { severity: unfocol::Severity::Warning, message: format!("Failed to set up watcher for colors.toml, Omarchy theme won't auto update: {e}")});
                        None
                }
            }
        });

        Ok(Box::new(Unfocol::new(config, config_dir, messages, watcher)))
        })
    )
}

fn is_not_found(e: &notify::Error) -> bool {
    match &e.kind {
        notify::ErrorKind::PathNotFound => true,
        notify::ErrorKind::Io(io) => io.kind() == std::io::ErrorKind::NotFound,
        _ => false,
    }
}
