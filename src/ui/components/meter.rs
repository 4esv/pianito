//! Cents deviation meter component.

use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::ui::theme::{BoxChars, Theme};

/// Cents deviation meter for visualizing pitch accuracy.
pub struct Meter {
    /// Current cents deviation from target (-50 to +50 range displayed).
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

impl Widget for Meter {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 5 || area.width < 20 {
            return; // Not enough space
        }

        // Meter scale: -50 to +50 cents
        let scale_range = 100.0; // -50 to +50
        let center_x = area.x + area.width / 2;

        // Draw scale labels
        let label_y = area.y;
        let labels = [
            (-50, format!("{} -50", BoxChars::FLAT)),
            (-25, "-25".to_string()),
            (-10, "-10".to_string()),
            (0, "0".to_string()),
            (10, "+10".to_string()),
            (25, "+25".to_string()),
            (50, format!("+50 {}", BoxChars::SHARP)),
        ];

        for (cents, label) in labels {
            let x_offset = (cents as f32 / scale_range) * (area.width as f32 - 2.0);
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

        for row in 0..meter_height {
            let y = meter_y_start + row;

            // Draw tick marks
            for tick_cents in [-50, -25, -10, 0, 10, 25, 50] {
                let x_offset = (tick_cents as f32 / scale_range) * (area.width as f32 - 2.0);
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

        // Draw the indicator if detecting
        if self.detecting {
            let clamped_cents = self.cents.clamp(-50.0, 50.0);
            let x_offset = (clamped_cents / scale_range) * (area.width as f32 - 2.0);
            let indicator_x = (center_x as f32 + x_offset) as u16;

            let style = Theme::style_for_cents(self.cents);

            // Draw vertical bar at the indicator position
            for row in 0..meter_height {
                let y = meter_y_start + row;
                if indicator_x >= area.x && indicator_x < area.x + area.width {
                    buf.set_string(indicator_x, y, "█", style);
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

        // Draw background track
        for x in area.x..area.x + width {
            let char = if x == center { '|' } else { '-' };
            buf.set_string(x, area.y, char.to_string(), Theme::muted());
        }

        // Draw indicator
        let clamped = self.cents.clamp(-50.0, 50.0);
        let offset = (clamped / 50.0) * (width as f32 / 2.0);
        let indicator_x = (center as f32 + offset) as u16;

        if indicator_x >= area.x && indicator_x < area.x + width {
            let style = Theme::style_for_cents(self.cents);
            buf.set_string(indicator_x, area.y, "●", style);
        }
    }
}
