use std::fs::create_dir_all;
use crate::color::{Color, Stop, Theme};
use displaydoc::Display;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use directories::{BaseDirs, ProjectDirs};
use thiserror::Error;
use crate::settings::write_atomic;

/// The built-in themes, bundled at compile time from `assets/themes.toml`.
///
/// This is what gets written to disk the first time `themes.toml` doesn't
/// exist yet (see [`ensure_themes_toml_exists`]).
pub const DEFAULT_THEMES: &str = include_str!("../assets/themes.toml");
const DEFAULT_THEME_TOML: &str = include_str!("../assets/default_theme.toml");
/// The name reserved for the built-in default theme, which [`load_themes`]
/// always inserts, overwriting any user-defined theme with the same name.
pub const DEFAULT_THEME: &str = "default";
/// Path, relative to Omarchy's state directory, of the symlink that points
/// at the currently active Omarchy theme's directory.
pub const OMARCHY_CURRENT: &str = "omarchy/current";

#[derive(Serialize, Deserialize)]
struct RawStop {
    color: String,
    progress: f32,
}

/// Why a raw, deserialized TOML stop failed to convert into a [`Stop`].
#[derive(Error, Debug)]
pub enum ParseStopError {
    #[error("Invalid progress value {value}")]
    InvalidProgressValue { value: f32 },
    #[error("Invalid hex value {hex_string}")]
    InvalidHexValue {
        hex_string: String,
        #[source]
        parse_color_error: csscolorparser::ParseColorError,
    },
}

impl TryFrom<RawStop> for Stop {
    type Error = ParseStopError;
    fn try_from(raw_stop: RawStop) -> Result<Self, Self::Error> {
        let color: Color =
            raw_stop
                .color
                .parse()
                .map_err(|cause| ParseStopError::InvalidHexValue {
                    hex_string: raw_stop.color,
                    parse_color_error: cause,
                })?;
        if !(0.0..=1.0).contains(&raw_stop.progress) {
            return Err(ParseStopError::InvalidProgressValue {
                value: raw_stop.progress,
            });
        }
        Ok(Stop {
            color,
            progress: raw_stop.progress,
        })
    }
}

#[derive(Deserialize)]
struct RawTheme {
    idle: String,
    #[serde(default)]
    clock_bg: Option<String>,
    #[serde(default)]
    clock_digits: Option<String>,
    stops: Vec<RawStop>,
}

/// Why a raw, deserialized TOML theme failed to convert into a [`Theme`].
#[derive(Error, Debug)]
pub enum ParseThemeError {
    #[error("Invalid color value for idle: {idle_hex}")]
    InvalidIdleValue {
        idle_hex: String,
        #[source]
        parse_color_error: csscolorparser::ParseColorError,
    },
    #[error("Invalid color value for clock background: {clock_bg_hex}")]
    InvalidClockBG {
        clock_bg_hex: String,
        #[source]
        parse_color_error: csscolorparser::ParseColorError,
    },
    #[error("Invalid color value for clock digits: {clock_digits_hex}")]
    InvalidClockDigits {
        clock_digits_hex: String,
        #[source]
        parse_color_error: csscolorparser::ParseColorError,
    },
    #[error("Invalid stop")]
    InvalidStop(#[from] ParseStopError),
    #[error("Stops must start at progress 0.0 and end at 1.0, found {first} and {last}")]
    InvalidProgressBounds { first: f32, last: f32 },
    #[error("Found {stop_2} following {stop_1}")]
    NonMonotonicStops { stop_1: f32, stop_2: f32 },
    #[error("Found {num_stops} stops")]
    TooFewStops { num_stops: usize },
}

impl TryFrom<RawTheme> for Theme {
    type Error = ParseThemeError;
    fn try_from(raw_theme: RawTheme) -> Result<Self, Self::Error> {
        let idle: Color =
            raw_theme
                .idle
                .parse()
                .map_err(|cause| ParseThemeError::InvalidIdleValue {
                    idle_hex: raw_theme.idle,
                    parse_color_error: cause,
                })?;

        let clock_bg: Color = raw_theme
            .clock_bg
            .map(|s| {
                s.parse().map_err(|cause| ParseThemeError::InvalidClockBG {
                    clock_bg_hex: s,
                    parse_color_error: cause,
                })
            })
            .transpose()?
            .unwrap_or(Color::BLACK);

        let clock_digits: Color = raw_theme
            .clock_digits
            .map(|s| {
                s.parse()
                    .map_err(|cause| ParseThemeError::InvalidClockDigits {
                        clock_digits_hex: s,
                        parse_color_error: cause,
                    })
            })
            .transpose()?
            .unwrap_or(Color::WHITE);

        let stops: Vec<Stop> = raw_theme
            .stops
            .into_iter()
            .map(Stop::try_from)
            .collect::<Result<Vec<Stop>, _>>()?;

        match stops.as_slice() {
            [first, .., last] => {
                if !(first.progress == 0.0 && last.progress == 1.0) {
                    return Err(ParseThemeError::InvalidProgressBounds {
                        first: first.progress,
                        last: last.progress,
                    });
                }
            }
            _ => {
                return Err(ParseThemeError::TooFewStops {
                    num_stops: stops.len(),
                });
            }
        }

        for window in stops.windows(2) {
            if window[1].progress <= window[0].progress {
                return Err(ParseThemeError::NonMonotonicStops {
                    stop_1: window[0].progress,
                    stop_2: window[1].progress,
                });
            }
        }

        Ok(Theme {
            idle,
            clock_bg,
            clock_digits,
            stops,
            interpolation_method: crate::color::InterpolationMethod::Lerp,
        })
    }
}

#[derive(Deserialize)]
struct RawConfig {
    themes: HashMap<String, RawTheme>,
}

/// When to show the remaining-time clock overlay.
#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub enum ShowClock {
    /// Never show the clock.
    Never,
    /// Always show the clock.
    Always,
    /// Only show the clock while the mouse is hovering the focus window.
    OnMouseOver,
}

impl std::fmt::Display for ShowClock {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let label = match self {
            ShowClock::Never => "Never",
            ShowClock::Always => "Always",
            ShowClock::OnMouseOver => "On mouse over",
        };
        write!(f, "{label}")
    }
}

/// The user's persisted preferences, read from and written to
/// `settings.toml`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Settings {
    pub show_settings: bool,
    pub show_clock: ShowClock,
    pub focus_time: u32,
    pub selected_theme: String,
    pub show_welcome_message: bool,
    pub show_time_is_up_message: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_settings: false,
            show_clock: ShowClock::OnMouseOver,
            focus_time: 25,
            selected_theme: DEFAULT_THEME.to_string(),
            show_welcome_message: false,
            show_time_is_up_message: false,
        }
    }
}

/// The application's full configuration: all loaded themes plus the user's
/// settings, including which theme is selected.
#[derive(Serialize)]
pub struct Config {
    pub themes: HashMap<String, Theme>,
    pub settings: Settings,
}

impl Config {
    /// Builds a [`Config`] from already-loaded themes and settings.
    pub fn new(themes: HashMap<String, Theme>, settings: Settings) -> Self {
        Self { themes, settings }
    }
}

impl Default for Config {
    fn default() -> Self {
        let themes: HashMap<String, Theme> =
            toml::from_str(DEFAULT_THEMES).expect("assets/themes.toml malformed");
        let settings = Settings::default();
        Self { themes, settings }
    }
}

/// Returns Unfocol's config directory, creating it if it doesn't exist yet.
///
/// # Panics
///
/// Panics if the OS has no usable home directory, or if the directory
/// couldn't be created.
pub fn get_or_create_config_dir() -> PathBuf {
    if let Some(proj_dir) = ProjectDirs::from("se", "johnkinell", "Unfocol") {
        create_dir_all(proj_dir.config_dir()).expect("Could not create config directory");
        proj_dir.config_dir().to_path_buf()
    } else {
        panic!("No home directory")
    }
}

/// Writes [`DEFAULT_THEMES`] to `themes_toml` if no file exists there yet.
///
/// # Arguments
///
/// * `themes_toml` - Where the themes file should live.
///
/// # Errors
///
/// Returns an error if the file doesn't exist and couldn't be written.
pub fn ensure_themes_toml_exists(themes_toml: &Path) -> Result<(), anyhow::Error> {
    if !themes_toml.exists() {
        info!("No themes.toml file found, creating default.");
        let toml_str = DEFAULT_THEMES;
        write_atomic(toml_str, themes_toml)?;
        Ok(())
    } else {
        info!("Found themes.toml");
        Ok(())
    }
}


/// Why [`load_themes`] failed to read or parse a theme.
#[derive(Error, Debug)]
pub enum LoadThemesError {
    #[error("Failed to read themes.toml")]
    FileUnreadable {
        file: PathBuf,
        #[source]
        io_error: std::io::Error,
    },
    #[error("Failed to parse themes.toml")]
    ParseTomlFailed(#[from] toml::de::Error),
    #[error("Failed to parse {name}")]
    InvalidTheme {
        name: String,
        #[source]
        parse_theme_error: ParseThemeError,
    },
    #[error(transparent)]
    OmarchyRelated(#[from] OmarchyThemeError),
}

/// Loads all themes from the user's `themes.toml`, merges in the built-in
/// `default` theme, and adds an `"Omarchy"` theme when running under Omarchy.
///
/// Parsing is best-effort: a theme that fails to parse is skipped and its
/// error is collected rather than aborting the whole load, so the caller
/// always gets back whatever themes *did* parse successfully.
///
/// # Arguments
///
/// * `themes_toml` - Path to the `themes.toml` file to read.
///
/// # Returns
///
/// A tuple of:
/// * The loaded themes, keyed by name. This always contains at least the
///   `"default"` theme, since it is inserted after parsing (overwriting any
///   user-defined theme also named `"default"`).
/// * Any errors encountered while reading or parsing, e.g. an unreadable
///   file, invalid TOML, or an individual theme that failed validation.
///
/// # Examples
///
/// ```no_run
/// use unfocol::load_themes;
///
/// let (themes, errors) = load_themes("themes.toml");
/// for error in &errors {
///     eprintln!("Problem loading a theme: {error}");
/// }
/// assert!(themes.contains_key("default"));
/// ```
pub fn load_themes(
    themes_toml: impl AsRef<Path>,
) -> (HashMap<String, Theme>, Vec<LoadThemesError>) {
    let (mut themes, mut errors) = try_load_themes(&themes_toml);
    if themes.is_empty() && errors.is_empty() {
        info!("themes.toml was empty.");
    }
    if themes.contains_key(DEFAULT_THEME) {
        info!("Found user defined 'default' theme, replacing.");
    }
    themes.insert(DEFAULT_THEME.to_string(), default_theme());

    if let Some(colors_toml_path) = omarchy_colors_toml() {
        match omarchy_theme(colors_toml_path) {
            Ok(omarchy_theme) => { themes.insert("Omarchy".to_string(), omarchy_theme); },
            Err(OmarchyThemeError::FileUnreadable(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                //Do nothing, since this is expected on distros other than Omarchy.
            },
            Err(e) => {
                errors.push(LoadThemesError::OmarchyRelated(e));
            }
        }
    }

    (themes, errors)
}

fn try_load_themes(
    themes_toml: impl AsRef<Path>,
) -> (HashMap<String, Theme>, Vec<LoadThemesError>) {
    let toml_contents = match get_toml_as_str(&themes_toml) {
        Ok(toml_contents) => toml_contents,
        Err(e) => {
            return (
                HashMap::new(),
                vec![LoadThemesError::FileUnreadable {
                    file: themes_toml.as_ref().into(),
                    io_error: e,
                }],
            );
        }
    };

    parse_themes(toml_contents)
}

fn parse_themes(toml_contents: String) -> (HashMap<String, Theme>, Vec<LoadThemesError>) {
    let raw_config: RawConfig = match toml::from_str(&toml_contents) {
        Ok(raw_config) => raw_config,
        Err(e) => {
            return (HashMap::new(), vec![LoadThemesError::ParseTomlFailed(e)]);
        }
    };

    let mut themes: HashMap<String, Theme> = HashMap::new();
    let mut errors: Vec<LoadThemesError> = Vec::new();
    for (name, raw_theme) in raw_config.themes.into_iter() {
        match Theme::try_from(raw_theme) {
            Ok(theme) => {
                themes.insert(name, theme);
            }
            Err(e) => errors.push(LoadThemesError::InvalidTheme {
                name,
                parse_theme_error: e,
            }),
        };
    }

    (themes, errors)
}


fn default_theme() -> Theme {
    let (mut themes, errors) = parse_themes(DEFAULT_THEME_TOML.to_string());
    debug_assert!(errors.is_empty(), "default_theme.toml malformed {errors:?}");
    debug_assert_eq!(
        themes.len(),
        1,
        "default_theme.toml contains more than one theme"
    );
    themes
        .remove(DEFAULT_THEME)
        .expect("Default theme missing from default_theme.toml")
}

/// Returns the path to Omarchy's "current theme" symlink
/// (`OMARCHY_CURRENT`) inside the OS state directory, or `None` if there
/// is no usable state directory (e.g. no home directory).
///
/// This does not check whether the path actually exists — it will not, on
/// systems that aren't running Omarchy.
pub fn omarchy_current() -> Option<PathBuf> {
    Some(BaseDirs::new()?
        .state_dir()?
        .join(OMARCHY_CURRENT))
}

/// Returns the path to the active Omarchy theme's `colors.toml`, or `None`
/// if [`omarchy_current`] can't determine a state directory.
pub fn omarchy_colors_toml() -> Option<PathBuf> {
    let omarchy_colors_toml_path_str = "theme/colors.toml";
    Some(omarchy_current()?.join(omarchy_colors_toml_path_str))
}

/// Why [`omarchy_theme`] failed to build a theme from Omarchy's `colors.toml`.
#[derive(Error, Debug)]
pub enum OmarchyThemeError {
    #[error(transparent)]
    FileUnreadable(#[from] std::io::Error),
    #[error("Failed to parse colors.toml: {0}")]
    TomlParsing(#[from] toml::de::Error),
    #[error("Missing definition for color(s): {}", missing_colors.join(", "))]
    MissingColor { missing_colors: Vec<&'static str> },
    #[error("Parsing of hex value failed: {0}")]
    ColorParsing(#[from] csscolorparser::ParseColorError),
}

/// Builds a [`Theme`] from an Omarchy `colors.toml` file.
///
/// Maps a handful of Omarchy palette colors (falling back between a couple
/// of alternative names for each, e.g. `green`/`color2`) onto a fixed
/// four-stop gradient plus an idle color and clock colors.
///
/// # Arguments
///
/// * `path` - Path to the Omarchy theme's `colors.toml`.
///
/// # Errors
///
/// Returns an error if the file can't be read or parsed, or if any of the
/// required colors are missing or malformed.
pub fn omarchy_theme(path: PathBuf) -> Result<Theme, OmarchyThemeError> {
    let toml_str = get_toml_as_str(path)?;
    let colors_table: HashMap<String, String> = toml::from_str(&toml_str)?;
    let mut palette = Vec::new();
    let mut missing_colors = Vec::new();

    let colors_to_extract: Vec<(&str, &[&str])> = vec![
        ("green", &["green", "color2"]),
        ("yellow", &["yellow", "color3"]),
        ("red", &["red", "color1"]),
        ("cyan", &["cyan", "color6"]),
        ("clock_digits", &["foreground"]),
        ("clock_bg", &["background"]),
    ];

    for (name, theme_colors) in colors_to_extract {
        match return_color(&colors_table, theme_colors) {
            Ok(Some(color)) => palette.push(Some(color)),
            Ok(None) => missing_colors.push(name),
            Err(e) => return Err(e),
        }
    }

    if let [Some(green), Some(yellow), Some(red), Some(cyan), Some(clock_digits), Some(clock_bg)] = palette[..] {
        let stop_0 = Stop {
            progress: 0.0,
            color: green,
        };
        let stop_1 = Stop {
            progress: 0.5,
            color: yellow,
        };
        let stop_2 = Stop {
            progress: 0.833,
            color: red,
        };
        let stop_3 = Stop {
            progress: 1.0,
            color: Color::BLACK,
        };
        let idle = cyan;
        let stops = vec![stop_0, stop_1, stop_2, stop_3];
        Ok(Theme {
            stops,
            idle,
            clock_bg,
            clock_digits,
            interpolation_method: crate::color::InterpolationMethod::Lerp,
        })
    } else {
        Err(OmarchyThemeError::MissingColor { missing_colors })
    }


}

fn return_color(colors_table: &HashMap<String, String>, color_names: &[&str]) -> Result<Option<Color>, OmarchyThemeError> {
    for color_name in color_names {
        if let Some(hex) = colors_table.get(*color_name) {
            let color: Color = hex.parse()?;
            return Ok(Some(color))
        }
    }
    Ok(None)
}

/// Why [`load_settings`] failed to read or parse `settings.toml`.
#[derive(Error, Debug)]
pub enum LoadSettingsError {
    #[error("Failed to read settings.toml")]
    FileUnreadable(#[from] std::io::Error),
    #[error("Failed to parse settings.toml")]
    ParseTomlFailed(#[from] toml::de::Error),
}

/// A single automatic fix applied by [`sanitize_selected_theme`] or
/// [`load_settings`] when a loaded setting was out of range or otherwise
/// invalid. Each variant's `Display` message (via `displaydoc`) is shown
/// to the user.
#[derive(Debug, Display)]
pub enum SettingsCorrection {
    /// The focus_time value was out of bounds ({value}), corrected to 25.
    InvalidFocusTime { value: u32 },
    /// The theme '{non_existing_theme}' was not found, default theme selected.
    InvalidSelectedTheme { non_existing_theme: String },
}

/// What happened while [`load_settings`] tried to load `settings.toml`,
/// for building a startup [`Message`](crate::Message) if one is warranted.
pub enum SettingsLoadingOutcome {
    /// `settings.toml` was read and parsed, possibly with some
    /// `SettingsCorrection`s applied to bring values into range.
    ParsedAndLoaded {
        corrections: Vec<SettingsCorrection>,
    },
    /// `settings.toml` existed but couldn't be read or parsed, so defaults
    /// were used instead.
    Defaulted {
        load_settings_error: LoadSettingsError,
    },
    /// `settings.toml` didn't exist yet, i.e. this is the first run.
    FirstRun,
}

/// Loads settings from `settings_toml`, falling back to [`Settings::default`]
/// if the file is missing, unreadable, or fails to parse.
///
/// Loaded settings are also sanitized (e.g. an out-of-range `focus_time` is
/// corrected), whereas defaulted or first-run settings need no sanitizing
/// since they're already valid.
///
/// # Arguments
///
/// * `settings_toml` - Path to the settings file to read.
///
/// # Returns
///
/// A tuple of the settings to use, and a [`SettingsLoadingOutcome`]
/// describing what happened, for building a startup message from.
pub fn load_settings(settings_toml: impl AsRef<Path>) -> (Settings, SettingsLoadingOutcome) {
    match try_load_settings(settings_toml) {
        Ok(settings) => {
            let (settings, corrections) = sanitize_settings(settings);
            (
                settings,
                SettingsLoadingOutcome::ParsedAndLoaded { corrections },
            )
        }
        Err(LoadSettingsError::FileUnreadable(io_error))
            if io_error.kind() == std::io::ErrorKind::NotFound =>
        {
            info!("First run detected.");
            (Settings::default(), SettingsLoadingOutcome::FirstRun)
        }
        Err(load_settings_error) => (
            Settings::default(),
            SettingsLoadingOutcome::Defaulted {
                load_settings_error,
            },
        ),
    }
}

fn try_load_settings(settings_toml: impl AsRef<Path>) -> Result<Settings, LoadSettingsError> {
    let toml_as_str = match get_toml_as_str(settings_toml) {
        Ok(toml_as_str) => toml_as_str,
        Err(e) => return Err(LoadSettingsError::FileUnreadable(e)),
    };

    parse_settings(toml_as_str)
}

fn parse_settings(toml_as_str: String) -> Result<Settings, LoadSettingsError> {
    match toml::from_str(&toml_as_str) {
        Ok(settings) => Ok(settings),
        Err(e) => Err(LoadSettingsError::ParseTomlFailed(e)),
    }
}

fn sanitize_settings(mut settings: Settings) -> (Settings, Vec<SettingsCorrection>) {
    match settings.focus_time {
        1..=99 => (settings, vec![]),
        _ => {
            let value = settings.focus_time;
            settings.focus_time = 25;
            (
                settings,
                vec![SettingsCorrection::InvalidFocusTime { value }],
            )
        }
    }
}

/// Falls back to [`DEFAULT_THEME`] if `settings.selected_theme` doesn't name
/// a theme actually present in `themes`.
///
/// # Arguments
///
/// * `settings` - The settings to check and, if needed, correct.
/// * `themes` - The themes that were actually loaded.
///
/// # Returns
///
/// The (possibly corrected) settings, and a `SettingsCorrection` if a
/// correction was needed.
pub fn sanitize_selected_theme(
    mut settings: Settings,
    themes: &HashMap<String, Theme>,
) -> (Settings, Option<SettingsCorrection>) {
    if !themes.contains_key(&settings.selected_theme) {
        let non_existing_theme = settings.selected_theme.clone();
        settings.selected_theme = DEFAULT_THEME.to_string();
        let correction = SettingsCorrection::InvalidSelectedTheme { non_existing_theme };
        (settings, Some(correction))
    } else {
        (settings, None)
    }
}

fn get_toml_as_str(path: impl AsRef<Path>) -> Result<String, std::io::Error> {
    let path = path.as_ref();
    fs::read_to_string(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_themes() -> HashMap<String, Theme> {
    let (themes, errors) = parse_themes(DEFAULT_THEMES.to_string());
    debug_assert!(
        errors.is_empty(),
        "Default themes did not parse correctly {errors:?}"
    );
    themes
    }

    fn default_theme_as_str() -> String {
        r##"
        [themes.default]
        idle = "#00cccc"

        [[themes.default.stops]]
        progress = 0.0
        color = "#00cc00"

        [[themes.default.stops]]
        progress = 0.5
        color = "#cccc00"

        [[themes.default.stops]]
        progress = 0.833
        color = "#cc0000"

        [[themes.default.stops]]
        progress = 1.0
        color = "#000000"
        "##
        .into()
    }
    #[test]
    fn load_themes_returns_valid_default_theme() {
        let toml_contents = default_theme_as_str();
        let (themes, errors) = parse_themes(toml_contents);
        let theme_name = DEFAULT_THEME.to_string();

        let correct_idle = Color {
            r: 0,
            g: 204,
            b: 204,
        };
        let correct_green = Color { r: 0, g: 204, b: 0 };
        let correct_yellow = Color {
            r: 204,
            g: 204,
            b: 0,
        };
        let correct_red = Color { r: 204, g: 0, b: 0 };
        let correct_black = Color { r: 0, g: 0, b: 0 };

        let correct_stop_0_progress: f32 = 0.0;
        let correct_stop_1_progress: f32 = 0.5;
        let correct_stop_2_progress: f32 = 0.833;
        let correct_stop_3_progress: f32 = 1.0;

        assert_eq!(themes[&theme_name].idle, correct_idle);
        assert_eq!(themes[&theme_name].stops[0].color, correct_green);
        assert_eq!(themes[&theme_name].stops[1].color, correct_yellow);
        assert_eq!(themes[&theme_name].stops[2].color, correct_red);
        assert_eq!(themes[&theme_name].stops[3].color, correct_black);

        assert_eq!(
            themes[&theme_name].stops[0].progress,
            correct_stop_0_progress
        );
        assert_eq!(
            themes[&theme_name].stops[1].progress,
            correct_stop_1_progress
        );
        assert_eq!(
            themes[&theme_name].stops[2].progress,
            correct_stop_2_progress
        );
        assert_eq!(
            themes[&theme_name].stops[3].progress,
            correct_stop_3_progress
        );
        assert!(errors.is_empty());
    }

    #[cfg(test)]
    fn valid_raw_theme() -> RawTheme {
        RawTheme {
            idle: "#00cccc".into(),
            stops: vec![
                RawStop {
                    progress: 0.0,
                    color: "#00cc00".into(),
                },
                RawStop {
                    progress: 1.0,
                    color: "#000000".into(),
                },
            ],
            clock_bg: Some("#000000".into()),
            clock_digits: Some("#ffffff".into()),
        }
    }

    #[test]
    fn missing_clock_bg_returns_black() {
        let raw = RawTheme {
            clock_bg: None,
            ..valid_raw_theme()
        };
        let theme = Theme::try_from(raw).unwrap();

        assert_eq!(theme.clock_bg, Color::BLACK);
    }

    #[test]
    fn missing_clock_digits_returns_white() {
        let raw = RawTheme {
            clock_digits: None,
            ..valid_raw_theme()
        };
        let theme = Theme::try_from(raw).unwrap();

        assert_eq!(theme.clock_digits, Color::WHITE);
    }

    #[test]
    fn malformed_clock_bg_produces_correct_error_variant() {
        let raw = RawTheme {
            clock_bg: Some("#0000000".into()),
            ..valid_raw_theme()
        };

        assert!(matches!(
            Theme::try_from(raw),
            Err(ParseThemeError::InvalidClockBG { .. })
        ));
    }

    #[test]
    fn malformed_clock_digits_produces_correct_error_variant() {
        let raw = RawTheme {
            clock_digits: Some("#fffffff".into()),
            ..valid_raw_theme()
        };

        assert!(matches!(
            Theme::try_from(raw),
            Err(ParseThemeError::InvalidClockDigits { .. })
        ));
    }

    #[test]
    fn malformed_idle_color_produces_correct_error_variant() {
        let raw = RawTheme {
            idle: "#0000000".into(),
            ..valid_raw_theme()
        };

        assert!(matches!(
            Theme::try_from(raw),
            Err(ParseThemeError::InvalidIdleValue { .. })
        ));
    }

    #[test]
    fn invalid_progress_bounds_in_stops_produces_correct_error_variant() {
        let raw = RawTheme {
            stops: vec![
                RawStop {
                    progress: 1.0,
                    color: "#00cc00".into(),
                },
                RawStop {
                    progress: 0.0,
                    color: "#000000".into(),
                },
            ],
            ..valid_raw_theme()
        };

        assert!(matches!(
            Theme::try_from(raw),
            Err(ParseThemeError::InvalidProgressBounds { .. })
        ));
    }

    #[test]
    fn non_monotonic_stops_produces_correct_error_variant() {
        let raw = RawTheme {
            stops: vec![
                RawStop {
                    progress: 0.0,
                    color: "#000000".into(),
                },
                RawStop {
                    progress: 0.7,
                    color: "#005500".into(),
                },
                RawStop {
                    progress: 0.5,
                    color: "#00dd00".into(),
                },
                RawStop {
                    progress: 1.0,
                    color: "#110000".into(),
                },
            ],
            ..valid_raw_theme()
        };

        assert!(matches!(
            Theme::try_from(raw),
            Err(ParseThemeError::NonMonotonicStops { .. })
        ));
    }

    #[test]
    fn too_few_stops_produces_correct_error_variant() {
        let raw = RawTheme {
            stops: vec![RawStop {
                progress: 1.0,
                color: "#00cc00".into(),
            }],
            ..valid_raw_theme()
        };

        assert!(matches!(
            Theme::try_from(raw),
            Err(ParseThemeError::TooFewStops { .. })
        ));
    }

    #[cfg(test)]
    fn valid_raw_stop() -> RawStop {
        RawStop {
            progress: 0.5,
            color: "#555555".into(),
        }
    }

    #[test]
    fn valid_raw_stop_returns_valid_stop() {
        let raw_stop = valid_raw_stop();

        assert!(matches!(
            Stop::try_from(raw_stop),
            Ok(Stop {
                progress: 0.5,
                color: Color {
                    r: 85, // 55_16 = 5 * 16 + 5 * 1 = 85
                    g: 85,
                    b: 85
                }
            })
        ));
    }

    #[test]
    fn raw_stop_progress_value_too_high_produces_correct_error_variant() {
        let raw = RawStop {
            progress: 1.1,
            ..valid_raw_stop()
        };

        assert!(matches!(
            Stop::try_from(raw),
            Err(ParseStopError::InvalidProgressValue { .. })
        ));
    }

    #[test]
    fn raw_stop_progress_value_negative_produces_correct_error_variant() {
        let raw = RawStop {
            progress: -0.5,
            ..valid_raw_stop()
        };

        assert!(matches!(
            Stop::try_from(raw),
            Err(ParseStopError::InvalidProgressValue { .. })
        ));
    }

    fn valid_themes_toml() -> String {
        r##"
        [themes.forest]
        idle = "#003300"

        [[themes.forest.stops]]
        progress = 0.0
        color = "#00cc00"

        [[themes.forest.stops]]
        progress = 1.0
        color = "#000000"

        [themes.ocean]
        idle = "#001a33"

        [[themes.ocean.stops]]
        progress = 0.0
        color = "#0088cc"

        [[themes.ocean.stops]]
        progress = 1.0
        color = "#000011"
        "##
        .to_string()
    }

    fn one_bad_theme_toml() -> String {
        r##"
        [themes.forest]
        idle = "#003300"

        [[themes.forest.stops]]
        progress = 0.0
        color = "not-a-color"

        [[themes.forest.stops]]
        progress = 1.0
        color = "#000000"

        [themes.ocean]
        idle = "#001a33"

        [[themes.ocean.stops]]
        progress = 0.0
        color = "#0088cc"

        [[themes.ocean.stops]]
        progress = 1.0
        color = "#000011"
        "##
        .to_string()
    }

    fn invalid_themes_toml() -> String {
        r##"
        [themes.forest]
        idle = "#003300"

        [themes.forest.stops]
        progress = 0.0
        color = "#00cc00"
        "##
        .to_string()
    }

    fn empty_themes_toml() -> String {
        "
        [themes]
        "
        .to_string()
    }

    fn all_invalid_themes_toml() -> String {
        r##"
        [themes.forest]
        idle = "not-a-color"

        [[themes.forest.stops]]
        progress = 0.0
        color = "#00cc00"

        [themes.ocean]
        idle = "#001a33"

        [[themes.ocean.stops]]
        progress = 1.1
        color = "#000011"
        "##
        .to_string()
    }

    #[test]
    fn valid_themes_toml_parses_correctly() {
        let toml_contents = valid_themes_toml();
        let (themes, errors) = parse_themes(toml_contents);

        assert_eq!(themes.len(), 2);
        assert!(errors.is_empty());
    }

    #[test]
    fn one_bad_theme_returns_one_theme_and_invalid_theme_error() {
        let toml_contents = one_bad_theme_toml();
        let (themes, errors) = parse_themes(toml_contents);

        assert_eq!(themes.len(), 1);
        assert!(matches!(errors[0], LoadThemesError::InvalidTheme { .. }));
    }

    #[test]
    fn invalid_themes_toml_returns_empty_themes_and_invalid_theme_error() {
        let toml_contents = invalid_themes_toml();
        let (themes, errors) = parse_themes(toml_contents);

        assert!(themes.is_empty());
        assert!(matches!(errors[0], LoadThemesError::ParseTomlFailed { .. }));
    }

    #[test]
    fn empty_themes_toml_returns_empty_themes_and_empty_errors() {
        let toml_contents = empty_themes_toml();
        let (themes, errors) = parse_themes(toml_contents);

        assert!(themes.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn all_invalid_themes_returns_empty_themes_and_two_errors() {
        let toml_contents = all_invalid_themes_toml();
        let (themes, errors) = parse_themes(toml_contents);

        assert!(themes.is_empty());
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn missing_themes_toml_file_returns_correct_error() {
        let themes_toml = "/no/valid/file/path.toml";
        let (themes, errors) = try_load_themes(themes_toml);

        assert!(themes.is_empty());
        assert!(matches!(errors[0], LoadThemesError::FileUnreadable { .. }));
    }

    fn invalid_settings_toml() -> String {
        r#"
        invalid_setting = "Invalid"
        "#
        .into()
    }

    #[test]
    fn invalid_settings_toml_produces_correct_error_variant() {
        let toml_as_str = invalid_settings_toml();
        let error = parse_settings(toml_as_str);

        assert!(matches!(error, Err(LoadSettingsError::ParseTomlFailed(..))));
    }

    fn valid_settings() -> Settings {
        Settings {
            show_settings: true,
            focus_time: 34,
            show_clock: ShowClock::Never,
            selected_theme: DEFAULT_THEME.into(),
            show_welcome_message: false,
            show_time_is_up_message: false,
        }
    }

    #[test]
    fn valid_settings_passes_sanitization() {
        let settings = valid_settings();
        let unchanged_settings = valid_settings();
        let (cleaned_settings, corrections) = sanitize_settings(settings);

        assert_eq!(cleaned_settings, unchanged_settings);
        assert_eq!(corrections.len(), 0);
    }

    #[test]
    fn zero_focus_time_gets_corrected() {
        let settings = Settings {
            focus_time: 0,
            ..valid_settings()
        };
        let _corrected_settings = Settings {
            focus_time: 25,
            ..valid_settings()
        };

        let (cleaned_settings, _) = sanitize_settings(settings);

        assert!(matches!(cleaned_settings, _corrected_settings));
    }

    #[test]
    fn zero_focus_time_returns_correct_error() {
        let settings = Settings {
            focus_time: 0,
            ..valid_settings()
        };
        let (_, corrections) = sanitize_settings(settings);

        assert_eq!(corrections.len(), 1);
        assert!(matches!(
            corrections[0],
            SettingsCorrection::InvalidFocusTime { .. }
        ));
    }

    #[test]
    fn too_high_focus_time_gets_corrected() {
        let settings = Settings {
            focus_time: 100,
            ..valid_settings()
        };
        let _corrected_settings = Settings {
            focus_time: 25,
            ..valid_settings()
        };

        assert!(matches!(settings, _corrected_settings));
    }

    #[test]
    fn too_high_focus_time_returns_correct_error() {
        let settings = Settings {
            focus_time: 100,
            ..valid_settings()
        };
        let (_, corrections) = sanitize_settings(settings);

        assert!(matches!(
            corrections[0],
            SettingsCorrection::InvalidFocusTime { .. }
        ));
        assert_eq!(corrections.len(), 1);
    }

    #[test]
    fn missing_settings_toml_file_returns_correct_error() {
        let settings_toml = "/no/valid/file/path.toml";
        let error = try_load_settings(settings_toml);

        assert!(matches!(error, Err(LoadSettingsError::FileUnreadable(..))));
    }

    fn one_valid_theme() -> Theme {
        Theme {
            idle: Color::IDLE,
            clock_bg: Color::BLACK,
            clock_digits: Color::WHITE,
            stops: crate::color::DEFAULT_STOPS.to_vec(),
            interpolation_method: crate::color::InterpolationMethod::Lerp,
        }
    }
    #[test]
    fn valid_selected_theme_passes_sanitization() {
        let mut themes = HashMap::new();
        let test_theme = "test_theme";
        themes.insert(test_theme.to_string(), one_valid_theme());
        let settings = Settings {
            selected_theme: test_theme.to_string(),
            ..Settings::default()
        };
        let (settings, correction) = sanitize_selected_theme(settings, &themes);

        assert!(correction.is_none());
        assert!(settings.selected_theme.contains(test_theme));
    }

    #[test]
    fn missing_selected_theme_gets_corrected() {
        let settings = Settings {
            selected_theme: "a_missing_theme".to_string(),
            ..valid_settings()
        };
        let themes = default_themes();
        let (settings, correction) = sanitize_selected_theme(settings, &themes);

        assert_eq!(settings.selected_theme, DEFAULT_THEME);
        assert!(correction.unwrap().to_string().contains("a_missing_theme"));
    }
}
