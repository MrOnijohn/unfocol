use crate::app::Unfocol;
use crate::config::ShowClock;
use eframe::egui::{self, Align2, Area, Frame, Id, Margin, Vec2};
use std::time::{Duration, Instant};

impl Unfocol<fn() -> Instant> {
    /// Draws the remaining-time overlay, according to `ShowClock`: never,
    /// always, or only while the mouse is hovering the window.
    pub fn render_clock(&self, ctx: &egui::Context) {
        match self.config.settings.show_clock {
            ShowClock::Never => {}
            ShowClock::OnMouseOver if !self.mouse_over => {}
            _ => {
                let time = self.timer.remaining();
                let clock = format_to_mmss(time);

                Area::new(Id::new("clock_overlay"))
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .interactable(false)
                    .show(ctx, |ui| {
                        Frame::new()
                            .fill(self.active_theme().clock_bg.into())
                            .inner_margin(Margin::symmetric(10, 7))
                            .corner_radius(4)
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(clock)
                                        .font(egui::FontId::monospace(20.0))
                                        .color(self.active_theme().clock_digits),
                                )
                            });
                    });
            }
        }
    }
}

fn format_to_mmss(time: Duration) -> String {
    let secs = time.as_secs() + (time.subsec_nanos() > 0) as u64;
    let minutes = secs / 60;
    let seconds = secs % 60;

    format!("{:02}:{:02}", minutes, seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_minutes_thirtyfour_seconds_returns_12_34() {
        let time = Duration::from_mins(12) + Duration::from_secs(34);
        let clock = format_to_mmss(time);

        assert_eq!(clock, "12:34".to_string());
    }

    #[test]
    fn five_seconds_returns_00_05() {
        let time = Duration::from_secs(5);
        let clock = format_to_mmss(time);

        assert_eq!(clock, "00:05".to_string())
    }

    #[test]
    fn one_minute_five_seconds_returns_01_05() {
        let time = Duration::from_mins(1) + Duration::from_secs(5);
        let clock = format_to_mmss(time);

        assert_eq!(clock, "01:05".to_string());
    }

    #[test]
    fn fiftynine_seconds_returns_00_59() {
        let time = Duration::from_secs(59);
        let clock = format_to_mmss(time);

        assert_eq!(clock, "00:59".to_string());
    }

    #[test]
    fn one_minute_returns_01_00() {
        let time = Duration::from_mins(1);
        let clock = format_to_mmss(time);

        assert_eq!(clock, "01:00".to_string());
    }

    #[test]
    fn zero_returns_00_00() {
        let time = Duration::ZERO;
        let clock = format_to_mmss(time);

        assert_eq!(clock, "00:00".to_string());
    }

    #[test]
    fn ninentynineminutes_returns_99_00() {
        let time = Duration::from_mins(99);
        let clock = format_to_mmss(time);

        assert_eq!(clock, "99:00".to_string());
    }

    #[test]
    fn just_above_zero_returns_00_01() {
        let time = Duration::ZERO + Duration::from_nanos(1);
        let clock = format_to_mmss(time);

        assert_eq!(clock, "00:01".to_string());
    }

    #[test]
    fn just_above_ninetyeight_fiftynine_returns_99_00() {
        let time = Duration::from_mins(98) + Duration::from_secs(59) + Duration::from_nanos(1);
        let clock = format_to_mmss(time);

        assert_eq!(clock, "99:00".to_string());
    }
}
