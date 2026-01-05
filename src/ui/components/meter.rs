//! Cents deviation meter component.

use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::ui::theme::{BoxChars, Theme};

/// Cents deviation meter for visualizing pitch accuracy.
/// Uses logarithmic scale for ±500 cents with a fixed "in-tune" zone at center.
pub struct Meter {
    /// Current cents deviation from target (±500 cents range, logarithmic scale).
    cents: f32,
    /// Whether we're currently detecting a pitch.
    detecting: bool,
    /// Tolerance threshold in cents.
    tolerance: f32,
}

impl Meter {
    /// Create a new meter.
    pub fn new(cents: f32) -> Self {
        Self {
            cents,
            detecting: true,
            tolerance: 5.0,
        }
    }

    /// Create a meter in "listening" state (no pitch detected).
    pub fn listening() -> Self {
        Self {
            cents: 0.0,
            detecting: false,
            tolerance: 5.0,
        }
    }

    /// Set the tolerance threshold.
    pub fn tolerance(mut self, tolerance: f32) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Set whether we're detecting.
    pub fn detecting(mut self, detecting: bool) -> Self {
        self.detecting = detecting;
        self
    }
}

impl Meter {
    /// Convert cents to screen position using logarithmic scale.
    /// Values within ±tolerance return 0 (center).
    /// Values outside use log scale: more resolution near center, compressed at edges.
    pub fn log_position(cents: f32, max_cents: f32, half_width: f32, tolerance: f32) -> f32 {
        if cents.abs() <= tolerance {
            return 0.0;
        }

        let sign = cents.signum();
        let abs_cents = cents.abs();

        // Logarithmic mapping: log(cents/tolerance) / log(max/tolerance)
        // This maps tolerance -> 0, max_cents -> 1
        let normalized = (abs_cents / tolerance).ln() / (max_cents / tolerance).ln();

        sign * normalized.clamp(0.0, 1.0) * half_width
    }
}

impl Widget for Meter {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 5 || area.width < 20 {
            return; // Not enough space
        }

        let center_x = area.x + area.width / 2;
        let half_width = (area.width / 2 - 1) as f32;
        let max_cents = 500.0;

        // Draw scale labels (logarithmically spaced)
        let label_y = area.y;
        let labels: [(i32, String); 7] = [
            (-500, format!("{} -5", BoxChars::FLAT)),
            (-100, "-1".to_string()),
            (-50, "".to_string()),
            (0, "0".to_string()),
            (50, "".to_string()),
            (100, "+1".to_string()),
            (500, format!("+5 {}", BoxChars::SHARP)),
        ];

        for (cents, label) in labels {
            if label.is_empty() {
                continue;
            }
            let x_offset = Self::log_position(cents as f32, max_cents, half_width, self.tolerance);
            let x = (center_x as f32 + x_offset) as u16;
            if x >= area.x && x + label.len() as u16 <= area.x + area.width {
                let style = if cents == 0 {
                    Theme::accent()
                } else {
                    Theme::muted()
                };
                buf.set_string(
                    x.saturating_sub(label.len() as u16 / 2),
                    label_y,
                    &label,
                    style,
                );
            }
        }

        // Draw meter lines
        let meter_y_start = area.y + 2;
        let meter_height = area.height.saturating_sub(4).min(5);

        // Draw tick marks at logarithmic positions
        let tick_values = [-500, -100, -50, -15, 0, 15, 50, 100, 500];
        for row in 0..meter_height {
            let y = meter_y_start + row;

            for &tick_cents in &tick_values {
                let x_offset =
                    Self::log_position(tick_cents as f32, max_cents, half_width, self.tolerance);
                let x = (center_x as f32 + x_offset) as u16;
                if x >= area.x && x < area.x + area.width {
                    let char = if tick_cents == 0 {
                        BoxChars::THICK_VERTICAL
                    } else {
                        BoxChars::THIN_VERTICAL
                    };
                    let style = if tick_cents == 0 {
                        Theme::accent()
                    } else {
                        Theme::muted()
                    };
                    buf.set_string(x, y, char.to_string(), style);
                }
            }
        }

        // Fixed in-tune zone width (in characters)
        let in_tune_zone_width: u16 = 7;

        // Draw the indicator if detecting
        if self.detecting {
            let style = Theme::style_for_cents(self.cents);

            if self.cents.abs() <= self.tolerance {
                // Within tolerance: draw fixed, wide green zone at center (no movement)
                let half_zone = in_tune_zone_width / 2;
                let start_x = center_x.saturating_sub(half_zone).max(area.x);
                let end_x = (center_x + half_zone + 1).min(area.x + area.width);

                for row in 0..meter_height {
                    let y = meter_y_start + row;
                    for x in start_x..end_x {
                        buf.set_string(x, y, "█", style);
                    }
                }
            } else {
                // Outside tolerance: narrow indicator at logarithmic position
                let clamped_cents = self.cents.clamp(-max_cents, max_cents);
                let x_offset =
                    Self::log_position(clamped_cents, max_cents, half_width, self.tolerance);
                let indicator_x = (center_x as f32 + x_offset) as u16;

                // Narrow indicator (1-2 chars) when out of tune
                for row in 0..meter_height {
                    let y = meter_y_start + row;
                    if indicator_x >= area.x && indicator_x < area.x + area.width {
                        buf.set_string(indicator_x, y, "█", style);
                    }
                }
            }

            // Draw cents value below meter
            let cents_text = format!("{:+.1} cents", self.cents);
            let cents_x = center_x.saturating_sub(cents_text.len() as u16 / 2);
            let cents_y = meter_y_start + meter_height;
            buf.set_string(cents_x, cents_y, &cents_text, style);

            // Draw direction hint if significantly off
            if self.cents.abs() > self.tolerance {
                let hint = if self.cents < 0.0 {
                    format!("{} Tighten", BoxChars::RIGHT_ARROW)
                } else {
                    format!("Loosen {}", BoxChars::LEFT_ARROW)
                };
                let hint_y = cents_y + 1;
                if hint_y < area.y + area.height {
                    let hint_x = center_x.saturating_sub(hint.len() as u16 / 2);
                    buf.set_string(hint_x, hint_y, &hint, style);
                }
            }
        } else {
            // Show "Listening..." message
            let msg = "Listening...";
            let msg_x = center_x.saturating_sub(msg.len() as u16 / 2);
            let msg_y = meter_y_start + meter_height / 2;
            buf.set_string(msg_x, msg_y, msg, Theme::muted());
        }
    }
}

/// Compact horizontal meter for use in smaller spaces.
pub struct CompactMeter {
    cents: f32,
    width: u16,
}

impl CompactMeter {
    /// Create a compact meter.
    pub fn new(cents: f32, width: u16) -> Self {
        Self { cents, width }
    }
}

impl Widget for CompactMeter {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 {
            return;
        }

        let width = self.width.min(area.width);
        let center = area.x + width / 2;
        let half_width = (width / 2) as f32;
        let max_cents = 500.0;
        let tolerance = 5.0;

        // Draw background track
        for x in area.x..area.x + width {
            let char = if x == center { '|' } else { '-' };
            buf.set_string(x, area.y, char.to_string(), Theme::muted());
        }

        // Draw indicator using logarithmic scale
        let style = Theme::style_for_cents(self.cents);
        let clamped = self.cents.clamp(-max_cents, max_cents);
        let offset = Meter::log_position(clamped, max_cents, half_width, tolerance);
        let indicator_x = (center as f32 + offset) as u16;

        if indicator_x >= area.x && indicator_x < area.x + width {
            buf.set_string(indicator_x, area.y, "●", style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_position_at_zero() {
        let pos = Meter::log_position(0.0, 500.0, 50.0, 5.0);
        assert_eq!(pos, 0.0);
    }

    #[test]
    fn test_log_position_within_tolerance() {
        let pos1 = Meter::log_position(3.0, 500.0, 50.0, 5.0);
        let pos2 = Meter::log_position(-4.0, 500.0, 50.0, 5.0);
        assert_eq!(pos1, 0.0);
        assert_eq!(pos2, 0.0);
    }

    #[test]
    fn test_log_position_at_tolerance_boundary() {
        let pos1 = Meter::log_position(5.0, 500.0, 50.0, 5.0);
        let pos2 = Meter::log_position(-5.0, 500.0, 50.0, 5.0);
        assert_eq!(pos1, 0.0);
        assert_eq!(pos2, 0.0);
    }

    #[test]
    fn test_log_position_symmetry() {
        let pos_pos = Meter::log_position(50.0, 500.0, 50.0, 5.0);
        let pos_neg = Meter::log_position(-50.0, 500.0, 50.0, 5.0);
        assert!(
            (pos_pos + pos_neg).abs() < 0.01,
            "Positions should be symmetric: {} and {}",
            pos_pos,
            pos_neg
        );
    }

    #[test]
    fn test_log_position_bounds() {
        let pos = Meter::log_position(1000.0, 500.0, 50.0, 5.0);
        assert!(pos.abs() <= 50.0);

        let neg = Meter::log_position(-1000.0, 500.0, 50.0, 5.0);
        assert!(neg.abs() <= 50.0);
    }

    #[test]
    fn test_log_position_at_max() {
        let pos = Meter::log_position(500.0, 500.0, 50.0, 5.0);
        assert!((pos - 50.0).abs() < 0.1);

        let neg = Meter::log_position(-500.0, 500.0, 50.0, 5.0);
        assert!((neg + 50.0).abs() < 0.1);
    }

    #[test]
    fn test_log_position_monotonic_positive() {
        let p1 = Meter::log_position(10.0, 500.0, 50.0, 5.0);
        let p2 = Meter::log_position(50.0, 500.0, 50.0, 5.0);
        let p3 = Meter::log_position(100.0, 500.0, 50.0, 5.0);
        let p4 = Meter::log_position(500.0, 500.0, 50.0, 5.0);

        assert!(p1 < p2, "{} should be < {}", p1, p2);
        assert!(p2 < p3, "{} should be < {}", p2, p3);
        assert!(p3 < p4, "{} should be < {}", p3, p4);
    }

    #[test]
    fn test_log_position_monotonic_negative() {
        let p1 = Meter::log_position(-10.0, 500.0, 50.0, 5.0);
        let p2 = Meter::log_position(-50.0, 500.0, 50.0, 5.0);
        let p3 = Meter::log_position(-100.0, 500.0, 50.0, 5.0);
        let p4 = Meter::log_position(-500.0, 500.0, 50.0, 5.0);

        assert!(p1 > p2, "{} should be > {}", p1, p2);
        assert!(p2 > p3, "{} should be > {}", p2, p3);
        assert!(p3 > p4, "{} should be > {}", p3, p4);
    }

    #[test]
    fn test_meter_new() {
        let meter = Meter::new(10.5);
        assert!((meter.cents - 10.5).abs() < 0.01);
        assert!(meter.detecting);
        assert_eq!(meter.tolerance, 5.0);
    }

    #[test]
    fn test_meter_listening() {
        let meter = Meter::listening();
        assert_eq!(meter.cents, 0.0);
        assert!(!meter.detecting);
        assert_eq!(meter.tolerance, 5.0);
    }

    #[test]
    fn test_meter_with_custom_tolerance() {
        let meter = Meter::new(0.0).tolerance(10.0);
        assert_eq!(meter.tolerance, 10.0);
    }

    #[test]
    fn test_meter_detecting_flag() {
        let meter = Meter::new(0.0).detecting(false);
        assert!(!meter.detecting);

        let meter = Meter::new(0.0).detecting(true);
        assert!(meter.detecting);
    }

    #[test]
    fn test_compact_meter_new() {
        let meter = CompactMeter::new(25.0, 80);
        assert!((meter.cents - 25.0).abs() < 0.01);
        assert_eq!(meter.width, 80);
    }

    #[test]
    fn test_log_position_different_tolerances() {
        let pos1 = Meter::log_position(10.0, 500.0, 50.0, 5.0);
        let pos2 = Meter::log_position(10.0, 500.0, 50.0, 10.0);

        // With tolerance=5, 10 cents is outside zone
        // With tolerance=10, 10 cents is inside zone (should be 0)
        assert!(pos1 > 0.0);
        assert_eq!(pos2, 0.0);
    }

    #[test]
    fn test_log_position_scaling() {
        // Test that half_width scales the output correctly
        let pos1 = Meter::log_position(100.0, 500.0, 25.0, 5.0);
        let pos2 = Meter::log_position(100.0, 500.0, 50.0, 5.0);

        // pos2 should be exactly 2x pos1
        assert!((pos2 / pos1 - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_log_position_edge_cases() {
        // Test exact tolerance boundary
        let pos = Meter::log_position(5.0001, 500.0, 50.0, 5.0);
        assert!(pos > 0.0, "Just above tolerance should be positive");

        let pos = Meter::log_position(-5.0001, 500.0, 50.0, 5.0);
        assert!(pos < 0.0, "Just below tolerance should be negative");
    }
}
