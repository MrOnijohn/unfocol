use eframe::egui;
use serde::{Deserialize, Serialize};

#[cfg(test)]
const STOP_0: Color = Color { r: 0, g: 204, b: 0 };
#[cfg(test)]
const STOP_1: Color = Color {
    r: 204,
    g: 204,
    b: 0,
};
#[cfg(test)]
const STOP_2: Color = Color { r: 204, g: 0, b: 0 };
#[cfg(test)]
const STOP_3: Color = Color { r: 0, g: 0, b: 0 };
#[cfg(test)]
pub const DEFAULT_STOPS: &[Stop] = &[
    Stop {
        color: STOP_0,
        progress: 0.0,
    },
    Stop {
        color: STOP_1,
        progress: 0.5,
    },
    Stop {
        color: STOP_2,
        progress: 0.833,
    },
    Stop {
        color: STOP_3,
        progress: 1.0,
    },
];

#[derive(Serialize, Deserialize, PartialEq, Debug, Copy, Clone)]
/// An RGB color, independent of any particular UI toolkit's color type.
///
/// Conversion to `egui::Color32` is provided via [`From`], and parsing from a
/// CSS-style hex/named color string (e.g. `"#00cc00"`) is provided via
/// [`std::str::FromStr`].
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0 };
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
    };
    pub const IDLE: Color = Color {
        r: 0,
        g: 204,
        b: 204,
    };
}

impl From<Color> for egui::Color32 {
    fn from(c: Color) -> Self {
        egui::Color32::from_rgb(c.r, c.g, c.b)
    }
}

impl std::str::FromStr for Color {
    type Err = csscolorparser::ParseColorError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let [r, g, b, _] = csscolorparser::parse(s)?.to_rgba8();
        Ok(Color { r, g, b })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
/// A single color stop in a [`Theme`]'s gradient.
///
/// `progress` is a value in `0.0..=1.0` marking how far through the focus
/// session this stop applies, and `color` is the color to use at that point.
/// A theme's stops are traversed in order and interpolated between,
/// according to its `InterpolationMethod`.
pub struct Stop {
    pub color: Color,
    pub progress: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
/// How a [`Theme`]'s color is computed between two [`Stop`]s.
///
/// Only [`InterpolationMethod::Lerp`] is currently implemented; the other
/// variants exist for future use and will panic if selected.
pub enum InterpolationMethod {
    /// Linear interpolation of the `r`, `g`, and `b` channels independently.
    Lerp,
    LinearRGB, // Not implemented
    Oklab,     // Not implemented
}

#[derive(Serialize, Deserialize, Clone)]
/// A named color scheme: the idle color shown before a focus session starts,
/// the clock overlay colors, and the gradient of [`Stop`]s the focus window
/// moves through as the session progresses.
pub struct Theme {
    pub stops: Vec<Stop>,
    pub idle: Color,
    pub clock_bg: Color,
    pub clock_digits: Color,
    pub interpolation_method: InterpolationMethod,
}

impl Theme {
    /// Builds a [`Theme`] from its parts.
    ///
    /// # Arguments
    ///
    /// * `stops` - The gradient stops, which should start at progress `0.0`
    ///   and end at `1.0`.
    /// * `idle` - The color shown while no focus session is running.
    /// * `clock_bg` - The clock overlay's background color.
    /// * `clock_digits` - The clock overlay's text color.
    /// * `interpolation_method` - How to interpolate between `stops`.
    pub fn new(
        stops: Vec<Stop>,
        idle: Color,
        clock_bg: Color,
        clock_digits: Color,
        interpolation_method: InterpolationMethod,
    ) -> Self {
        Self {
            stops,
            idle,
            clock_bg,
            clock_digits,
            interpolation_method,
        }
    }

    /// Returns the color for progress `t` through this theme's gradient.
    ///
    /// # Arguments
    ///
    /// * `t` - Progress through the focus session, in `0.0..=1.0`.
    ///
    /// # Panics
    ///
    /// Panics if `interpolation_method` is not `InterpolationMethod::Lerp`,
    /// since the other methods are not yet implemented.
    pub fn current_color(&self, t: f32) -> Color {
        match self.interpolation_method {
            InterpolationMethod::Lerp => Self::lerp(self, t),
            _ => todo!(),
        }
    }

    fn lerp(&self, t: f32) -> Color {
        if t == 1.0
            && let Some(stop) = self.stops.last()
        {
            return stop.color;
        }
        for i in 0..self.stops.len() - 1 {
            if self.stops[i].progress <= t && t < self.stops[i + 1].progress {
                let ratio: f32 = (t - self.stops[i].progress)
                    / (self.stops[i + 1].progress - self.stops[i].progress);
                let r: f32 = self.stops[i].color.r as f32
                    + ((self.stops[i + 1].color.r as f32 - self.stops[i].color.r as f32) * ratio);
                let g: f32 = self.stops[i].color.g as f32
                    + ((self.stops[i + 1].color.g as f32 - self.stops[i].color.g as f32) * ratio);
                let b: f32 = self.stops[i].color.b as f32
                    + ((self.stops[i + 1].color.b as f32 - self.stops[i].color.b as f32) * ratio);
                let lerped_color = Color {
                    r: r.round() as u8,
                    g: g.round() as u8,
                    b: b.round() as u8,
                };
                return lerped_color;
            }
        }
        // Should never happen, since t is clamped
        panic!(
            "lerp: no segment found for t={t:.4}, stops={:?}",
            self.stops
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid_theme() -> Theme {
        Theme::new(
            DEFAULT_STOPS.to_vec(),
            Color::IDLE,
            Color::BLACK,
            Color::WHITE,
            InterpolationMethod::Lerp,
        )
    }

    #[test]
    fn current_color_is_stop_0_at_t_0() {
        let theme = valid_theme();
        let t: f32 = 0.0;
        let stop_0 = theme.current_color(t);

        assert_eq!(stop_0, STOP_0);
    }

    #[test]
    fn current_color_is_stop_1_at_t_0_point_5() {
        let theme = valid_theme();
        let t: f32 = 0.5;
        let stop_1 = theme.current_color(t);

        assert_eq!(stop_1, STOP_1);
    }

    #[test]
    fn current_color_is_stop_2_at_t_0_point_8_3_3() {
        let theme = valid_theme();
        let t: f32 = 0.833;
        let stop_2 = theme.current_color(t);

        assert_eq!(stop_2, STOP_2);
    }

    #[test]
    fn current_color_is_correct_at_t_0_point_25() {
        let theme = valid_theme();
        let t: f32 = 0.25;
        let color = theme.current_color(t);
        let correct_color = Color {
            r: 102,
            g: 204,
            b: 0,
        };

        assert_eq!(color, correct_color);
    }

    #[test]
    fn current_color_is_correct_at_t_0_point_6_6_6() {
        let theme = valid_theme();
        let t: f32 = 0.666;
        let color = theme.current_color(t);
        let correct_color = Color {
            r: 204,
            g: 102,
            b: 0,
        };

        assert_eq!(color, correct_color);
    }

    #[test]
    fn current_color_is_correct_at_t_0_point_9_1_6_5() {
        let theme = valid_theme();
        let t: f32 = 0.9165;
        let color = theme.current_color(t);
        let correct_color = Color { r: 102, g: 0, b: 0 };

        assert_eq!(color, correct_color);
    }
}
