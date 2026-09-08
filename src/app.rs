use std::path::{PathBuf, Path};
use std::time::{Duration, Instant};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc::{channel, Receiver};

use eframe::egui;

use crate::Timer;
use crate::color::Color;
use crate::config::{Config, omarchy_colors_toml, omarchy_theme};
use crate::timer::SessionState;
use crate::{Message, Theme};

const DEBOUNCE: Duration = Duration::from_millis(150);

/// Watches Omarchy's `colors.toml` for changes and, once a burst of file
/// events settles, signals that the "Omarchy" theme should be rebuilt.
///
/// Events are debounced (see `DEBOUNCE`) since a theme switch touches the
/// file multiple times in quick succession.
pub struct OmarchyWatcher {
    // Held only so its Drop doesn't run — dropping it stops the watch.
    _watcher: RecommendedWatcher,
    rx: Receiver<()>,
    pending: Option<Instant>,

}

impl OmarchyWatcher {
    /// Starts watching `path` for changes, requesting a repaint on `ctx`
    /// whenever an event fires.
    ///
    /// # Arguments
    ///
    /// * `path` - The file or directory to watch, recursively.
    /// * `ctx` - The egui context to wake up on each change event.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying OS watch could not be set up, e.g.
    /// because `path` does not exist.
    pub fn new(path: &Path, ctx: egui::Context) -> Result<Self, notify::Error> {
        let (tx, rx) = channel();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok() && tx.send(()).is_ok() {
                ctx.request_repaint();
            }
        })?;

        watcher.watch(path, RecursiveMode::Recursive)?;

        Ok(Self { _watcher: watcher, rx, pending: None })
    }

    /// True when a burst of events has settled and the theme should be rebuilt.
    pub fn should_reload(&mut self) -> bool {
        let mut saw_event = false;
        while self.rx.try_recv().is_ok() {
            saw_event = true;
        }
        if saw_event {
            self.pending = Some(Instant::now());
        }

        match self.pending {
            Some(started) if started.elapsed() >= DEBOUNCE => {
                self.pending = None;
                true
            }
            _ => false,
        }
    }

    /// True while a debounce window is running, i.e. a change was seen but
    /// [`should_reload`](Self::should_reload) hasn't yet returned `true` for it.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

/// The application's top-level state, and the [`eframe::App`] that egui
/// drives each frame.
///
/// `F` is the clock function type used by `timer` — see [`Timer`] — and is
/// fixed to `fn() -> Instant` (the real system clock) outside of tests.
pub struct Unfocol<F: Fn() -> Instant> {
    pub timer: Timer<F>,
    pub settings_t: f32,
    pub settings_idle: bool,
    pub mouse_over: bool,
    pub config: Config,
    pub config_dir: PathBuf,
    pub messages: Vec<Message>,
    pub omarchy_watcher: Option<OmarchyWatcher>,
}

impl Unfocol<fn() -> Instant> {
    /// Builds the initial application state.
    ///
    /// # Arguments
    ///
    /// * `config` - The loaded themes and settings.
    /// * `config_dir` - Directory settings are saved back to.
    /// * `messages` - Any startup notifications (load errors, welcome
    ///   message, etc.) to show immediately.
    /// * `omarchy_watcher` - The Omarchy `colors.toml` watcher, if running
    ///   under Omarchy and one could be set up.
    pub fn new(config: Config, config_dir: PathBuf, messages: Vec<Message>, omarchy_watcher: Option<OmarchyWatcher>) -> Self {
        Self {
            timer: Timer::new(config.settings.focus_time),
            settings_t: 0.0,
            settings_idle: false,
            mouse_over: false,
            config,
            config_dir,
            messages,
            omarchy_watcher,
        }
    }

    fn handle_inputs(&mut self, ctx: &egui::Context) {
        let (toggle_state, open_settings, reset_timer, quit, mouse_over) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::Comma),
                i.key_pressed(egui::Key::R),
                i.key_pressed(egui::Key::Q),
                i.pointer.hover_pos().is_some(),
            )
        });
        if toggle_state {
            self.timer.toggle();
        }
        if open_settings {
            self.config.settings.show_settings = true;
        }
        if reset_timer {
            self.timer.reset();
        }
        if quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.mouse_over = mouse_over;
    }

    fn render_focus_window(&mut self, current_color: Color, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(current_color.into()))
            .show_inside(ui, |_ui| {});
    }

    fn get_current_color(&self) -> Color {
        if self.config.settings.show_settings {
            if self.settings_idle {
                self.active_theme().idle
            } else {
                self.active_theme().current_color(self.settings_t)
            }
        } else {
            match self.timer.state {
                SessionState::Idle { .. } => self.active_theme().idle,
                SessionState::Running { .. } => {
                    let t: f32 = self.timer.progress();
                    self.active_theme().current_color(t)
                }
            }
        }
    }

    /// Returns the currently selected [`Theme`].
    pub fn active_theme(&self) -> &Theme {
        &self.config.themes[&self.config.settings.selected_theme]
    }
}

impl eframe::App for Unfocol<fn() -> Instant> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_inputs(ui.ctx());

        if self.timer.remaining().is_zero() && self.config.settings.show_time_is_up_message {
            self.messages.push(Message::time_is_up());
        }

        self.timer.update_state();
        // Omarchy theme reload: decide first, so the watcher borrow is released
        // before anything else on `self` is mutated.
        let reload = match &mut self.omarchy_watcher {
            Some(watcher) => {
                let reload = watcher.should_reload();
                if !reload && watcher.is_pending() {
                    ui.ctx().request_repaint_after(DEBOUNCE);
                }
                reload
            }
            None => false,
        };

        if reload && let Some(path) = omarchy_colors_toml() {
            match omarchy_theme(path) {
                Ok(theme) => {
                    self.config.themes.insert("Omarchy".to_string(), theme);
                }
                Err(e) => {
                    self.messages.push(Message::from_omarchy_theme_error(e));
                }
            }
        }

        self.render_settings(ui.ctx());
        self.render_clock(ui.ctx());

        let current_color: Color = self.get_current_color();
        self.render_focus_window(current_color, ui);

        self.display_notifications(ui.ctx());

        let nanos = self.timer.remaining().subsec_nanos();
        let until_repaint = if nanos == 0 {
            Duration::from_secs(1)
       } else {
            Duration::from_nanos(nanos as u64)
        };
        if matches!(self.timer.state, SessionState::Running { .. }) {
            ui.ctx()
                .request_repaint_after(until_repaint + Duration::from_millis(1));
        }
    }
}
