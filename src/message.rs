
use crate::config::{
    LoadSettingsError, LoadThemesError, OmarchyThemeError, ParseStopError, ParseThemeError, SettingsCorrection, SettingsLoadingOutcome,
};

fn default_themes_loaded() -> &'static str {
    "Default themes have been loaded instead."
}

fn default_settings_loaded() -> &'static str {
    "Default settings have been used instead."
}

fn correct_progress_values_and_restart() -> &'static str {
    "Correct progress values and restart to try again."
}

fn correct_hex_code_and_restart() -> &'static str {
    "Correct hex code and restart to try again."
}

/// How urgently a [`Message`] should be presented to the user.
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// A user-facing notification, shown in the notifications viewport (see
/// [`display_notifications`](crate::Unfocol::display_notifications)).
pub struct Message {
    pub severity: Severity,
    pub message: String,
}

impl Message {
    /// An informational message shown on first run (or whenever
    /// `show_welcome_message` is enabled), explaining the keyboard controls.
    pub fn welcome_message() -> Self {
        let message = "Welcome to Unfocol!\nPress Space to start or pause focus timer, R to reset, S for Settings and Q to quit.\nDisable this message in settings.".to_string();
        let severity = Severity::Info;
        Self { message, severity }
    }

    /// An informational message shown when a focus session's time runs out.
    pub fn time_is_up() -> Self {
        let message = "Focus time is up!".to_string();
        let severity = Severity::Info;
        Self { message, severity}
    }

    /// A warning shown when writing `settings.toml` to disk fails.
    pub fn save_settings_failed() -> Self {
        let message = "Failed to save settings to disk, your changes might not persist. Check free space and file permissions.".to_string();
        let severity = Severity::Warning;
        Self { severity, message }
    }

    /// Builds a user-facing message explaining a `LoadThemesError` returned
    /// by [`load_themes`](crate::load_themes), including advice on how to
    /// fix it where applicable.
    pub fn from_load_themes_error(error: LoadThemesError) -> Self {
        match error {
            LoadThemesError::FileUnreadable { file, io_error } => {
                let message = format!(
                    "Loading themes from {} failed with error {}.\n{}",
                    file.display(),
                    io_error,
                    default_themes_loaded(),
                );
                let severity = Severity::Error;
                Self { severity, message }
            }
            LoadThemesError::ParseTomlFailed(parsing_error) => {
                let message = format!(
                    "Parsing themes.toml failed with error \n{}\n{}",
                    parsing_error,
                    default_themes_loaded(),
                );
                let severity = Severity::Error;
                Self { severity, message }
            }
            LoadThemesError::InvalidTheme {
                name,
                parse_theme_error,
            } => {
                let (details, advice) = match &parse_theme_error {
                    ParseThemeError::InvalidIdleValue { .. }
                    | ParseThemeError::InvalidClockBG { .. }
                    | ParseThemeError::InvalidClockDigits { .. } => (
                        parse_theme_error.to_string(),
                        correct_hex_code_and_restart().to_string(),
                    ),
                    ParseThemeError::InvalidStop(parse_stop_error) => {
                        let details = format!("Invalid stop: {parse_stop_error}");
                        let advice = match parse_stop_error {
                            ParseStopError::InvalidProgressValue { .. } => {
                                correct_progress_values_and_restart().to_string()
                            }
                            ParseStopError::InvalidHexValue { .. } => {
                                correct_hex_code_and_restart().to_string()
                            }
                        };
                        (details, advice)
                    }
                    ParseThemeError::InvalidProgressBounds { .. } => (
                        parse_theme_error.to_string(),
                        correct_progress_values_and_restart().to_string(),
                    ),
                    ParseThemeError::NonMonotonicStops { .. } => (
                        parse_theme_error.to_string(),
                        correct_progress_values_and_restart().to_string(),
                    ),
                    ParseThemeError::TooFewStops { .. } => (
                        parse_theme_error.to_string(),
                        "Make sure that the theme has at least two stops.".to_string(),
                    ),
                };
                let message = format!(
                    "The theme \"{}\" in themes.toml did not parse correctly with error: {}\n{}",
                    name, details, advice
                );
                let severity = Severity::Warning;
                Self { severity, message }
            },
            LoadThemesError::OmarchyRelated(e) => {
                Self::from_omarchy_theme_error(e)
            }
        }
    }

    /// Builds a user-facing message from a
    /// [`SettingsLoadingOutcome`](crate::SettingsLoadingOutcome), if that
    /// outcome is worth telling the user about.
    ///
    /// Returns `None` for [`SettingsLoadingOutcome::FirstRun`] and for a
    /// successful load with no corrections, since there's nothing to report.
    pub fn from_settings_loading_outcome(outcome: SettingsLoadingOutcome) -> Option<Self> {
        match outcome {
            SettingsLoadingOutcome::ParsedAndLoaded { corrections } => {
                if corrections.is_empty() {
                    return None;
                }
                let details: String = corrections
                    .into_iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");

                let message = format!(
                    "Loading settings from settings.toml needed some correction(s):\n{details}"
                );
                let severity = Severity::Warning;
                Some(Self { message, severity })
            }
            SettingsLoadingOutcome::Defaulted {
                load_settings_error,
            } => {
                let message = match load_settings_error {
                    LoadSettingsError::FileUnreadable(error) => {
                        format!(
                            "Reading settings.toml returned the error\n{}\n{}",
                            error,
                            default_settings_loaded()
                        )
                    }
                    LoadSettingsError::ParseTomlFailed(error) => {
                        format!(
                            "Parsing the contents of settings.toml failed with the error\n{}\n{}",
                            error,
                            default_settings_loaded()
                        )
                    }
                };
                let severity = Severity::Error;
                Some(Self { message, severity })
            }
            SettingsLoadingOutcome::FirstRun => None,
        }
    }

    /// Builds a warning explaining a single `SettingsCorrection` that was
    /// applied to the user's settings.
    pub fn from_settings_correction(correction: SettingsCorrection) -> Self {
        Self {
            message: format!(
                "Loading settings from settings.toml needed a correction:\n{}",
                correction
            ),
            severity: Severity::Warning,
        }
    }

    /// Builds an error message explaining why loading the Omarchy theme
    /// failed.
    // TODO! Make the messages more friendly for the end user
    pub fn from_omarchy_theme_error(omarchy_theme_error: OmarchyThemeError) -> Self {
        let message = match omarchy_theme_error {
            OmarchyThemeError::FileUnreadable(error) => error.to_string(), 
            OmarchyThemeError::TomlParsing(error) => error.to_string(),
            OmarchyThemeError::ColorParsing(error) => error.to_string(),
            OmarchyThemeError::MissingColor { missing_colors } => format!("One or more vital colors were missing definitions in colors.toml: {}", missing_colors.join(", ")),
        };
        let severity = Severity::Error;
        Self { severity, message}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;

    #[test]
    fn missing_settings_toml_produces_correct_message() {
        let io_error =
            std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory");
        let load_settings_error = LoadSettingsError::FileUnreadable(io_error);
        let outcome = SettingsLoadingOutcome::Defaulted {
            load_settings_error,
        };

        let message = Message::from_settings_loading_outcome(outcome).unwrap();

        assert!(matches!(message.severity, Severity::Error));
        assert!(message.message.contains("Reading settings.toml"));
    }

    #[test]
    fn invalid_toml_produces_correct_message() {
        let toml_as_str = "invalid toml";
        let parse_toml_error = toml::from_str::<Settings>(toml_as_str).unwrap_err();
        let load_settings_error = LoadSettingsError::ParseTomlFailed(parse_toml_error);
        let outcome = SettingsLoadingOutcome::Defaulted {
            load_settings_error,
        };

        let message = Message::from_settings_loading_outcome(outcome).unwrap();

        assert!(matches!(message.severity, Severity::Error));
        assert!(message.message.contains("Parsing the contents"));
    }

    #[test]
    fn invalid_focus_time_produces_correct_message() {
        let value = 100;
        let corrections = vec![SettingsCorrection::InvalidFocusTime { value }];
        let outcome = SettingsLoadingOutcome::ParsedAndLoaded { corrections };

        let message = Message::from_settings_loading_outcome(outcome).unwrap();

        assert!(matches!(message.severity, Severity::Warning));
        assert!(message.message.contains("corrected to 25"))
    }

    #[test]
    fn missing_selected_theme_produces_correct_message() {
        let non_existing_theme = "cauliflower".to_string();
        let corrections = vec![SettingsCorrection::InvalidSelectedTheme { non_existing_theme }];
        let outcome = SettingsLoadingOutcome::ParsedAndLoaded { corrections };

        let message = Message::from_settings_loading_outcome(outcome).unwrap();

        assert!(matches!(message.severity, Severity::Warning));
        assert!(message.message.contains("cauliflower"));
    }
}
