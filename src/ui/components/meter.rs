//! Cents deviation meter component.

use ratatui::{buffer::Buffer, layout::Rect, style::Modifier, widgets::Widget};

use crate::ui::theme::{BoxChars, Theme};

/// Cents deviation meter for visualizing pitch accuracy.
/// Uses logarithmic scale for ±500 cents with a fixed "in-tune" zone at center.
pub struct Meter {
    /// Current cents deviation from target (±500 cents range, logarithmic scale).
    cents: f32,
    /// Smoothed cents position the needle is actually drawn at (issue #32:
    /// interpolated across frames instead of snapping to each raw ~10Hz
    /// reading). Defaults to `cents` so callers that never set it get the
    /// original snap-to-reading behavior.
    smoothed_cents: f32,
    /// Fading trail of recent smoothed positions, oldest first, each paired
    /// with a fade weight in `(0, 1]` (see [`super::animation::NeedleTrail`]).
    trail: Vec<(f32, f32)>,
    /// Whether we're currently detecting a pitch.
    detecting: bool,
    /// Tolerance threshold in cents.
    tolerance: f32,
    /// Whether the in-tune zone is mid lock-flash this frame (issue #32).
    flashing: bool,
}

impl Meter {
    /// Create a new meter.
    pub fn new(cents: f32) -> Self {
        Self {
            cents,
            smoothed_cents: cents,
            trail: Vec::new(),
            detecting: true,
            tolerance: 5.0,
            flashing: false,
        }
    }

    /// Create a meter in "listening" state (no pitch detected).
    pub fn listening() -> Self {
        Self {
            cents: 0.0,
            smoothed_cents: 0.0,
            trail: Vec::new(),
            detecting: false,
            tolerance: 5.0,
            flashing: false,
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

    /// Draw the needle at this smoothed position instead of the raw
    /// `cents` value (issue #32's needle smoothing).
    pub fn smoothed(mut self, smoothed_cents: f32) -> Self {
        self.smoothed_cents = smoothed_cents;
        self
    }

    /// Fading trail of recent smoothed positions to draw behind the needle
    /// (issue #32). Each entry is `(cents, fade_weight)`; see
    /// [`super::animation::NeedleTrail::trail`].
    pub fn trail(mut self, trail: impl IntoIterator<Item = (f32, f32)>) -> Self {
        self.trail = trail.into_iter().collect();
        self
    }

    /// Mark the in-tune zone as mid lock-flash this frame (issue #32's
    /// lock animation): drawn with an inverted pop instead of the normal
    /// fill.
    pub fn flashing(mut self, flashing: bool) -> Self {
        self.flashing = flashing;
        self
    }
}

impl Meter {
    /// Pivot (in cents) for the tick/label/out-of-zone log curve.
    ///
    /// Kept fixed, rather than pivoting on `tolerance` as the ticks used to,
    /// because pivoting on tolerance always maps `tolerance` itself to
    /// offset 0, which makes it impossible to derive a non-zero in-tune
    /// zone width from "where does tolerance land". A fixed cent value
    /// keeps ticks and the zone on the same curve, so they agree at any
    /// width or tolerance.
    const LOG_SCALE_PIVOT_CENTS: f32 = 1.0;

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

    /// Half-width (in the same units as `half_width`, i.e. character
    /// columns) of the in-tune zone, derived from `tolerance` instead of a
    /// fixed character count. This is literally "the +tolerance x-position"
    /// on the same pivoted log curve used for ticks, so the drawn zone edge
    /// lines up with wherever a `tolerance`-cents reading would otherwise be
    /// plotted.
    pub fn zone_half_width(tolerance: f32, max_cents: f32, half_width: f32) -> f32 {
        Self::log_position(
            tolerance,
            max_cents,
            half_width,
            Self::LOG_SCALE_PIVOT_CENTS,
        )
    }

    /// Map cents within tolerance linearly across the in-tune zone, so
    /// movement finer than tolerance still shows up as needle movement
    /// (sub-tolerance drift) instead of collapsing to a single dead-center
    /// point. Returns an offset in the same units as `zone_half_width`;
    /// callers add it to the zone's center x-position.
    pub fn sub_tolerance_offset(cents: f32, tolerance: f32, zone_half_width: f32) -> f32 {
        if tolerance <= 0.0 {
            return 0.0;
        }
        (cents / tolerance).clamp(-1.0, 1.0) * zone_half_width
    }

    /// Screen-column offset for `cents`: the linear in-zone curve within
    /// `tolerance`, or the logarithmic out-of-zone curve beyond it. Used to
    /// plot each historical trail sample (issue #32) on the same curve the
    /// live indicator uses, so a given cents value always lands at the same
    /// column whichever one is asking.
    fn position_offset(cents: f32, tolerance: f32, max_cents: f32, half_width: f32) -> f32 {
        if cents.abs() <= tolerance {
            let zone_half_width = Self::zone_half_width(tolerance, max_cents, half_width);
            Self::sub_tolerance_offset(cents, tolerance, zone_half_width)
        } else {
            let clamped = cents.clamp(-max_cents, max_cents);
            Self::log_position(clamped, max_cents, half_width, Self::LOG_SCALE_PIVOT_CENTS)
        }
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
            let x_offset = Self::log_position(
                cents as f32,
                max_cents,
                half_width,
                Self::LOG_SCALE_PIVOT_CENTS,
            );
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
                let x_offset = Self::log_position(
                    tick_cents as f32,
                    max_cents,
                    half_width,
                    Self::LOG_SCALE_PIVOT_CENTS,
                );
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
            let style = Theme::style_for_cents(self.cents);

            // Fading needle trail (issue #32): reserve the meter's top row
            // for it, but only when there's actually trail history to draw
            // and a spare row to give it - otherwise every row still goes
            // to the main indicator, unchanged from before this existed.
            let trail_rows: u16 = if !self.trail.is_empty() && meter_height >= 2 {
                1
            } else {
                0
            };
            if trail_rows > 0 {
                let trail_y = meter_y_start;
                for &(trail_cents, weight) in &self.trail {
                    let offset =
                        Self::position_offset(trail_cents, self.tolerance, max_cents, half_width);
                    let x = (center_x as f32 + offset)
                        .round()
                        .clamp(area.x as f32, (area.x + area.width - 1) as f32)
                        as u16;
                    let mut trail_style = Theme::style_for_cents(trail_cents);
                    if weight < 1.0 {
                        trail_style = trail_style.add_modifier(Modifier::DIM);
                    }
                    buf.set_string(
                        x,
                        trail_y,
                        BoxChars::block_for_fill(weight).to_string(),
                        trail_style,
                    );
                }
            }
            let indicator_start = meter_y_start + trail_rows;
            let indicator_end = meter_y_start + meter_height;

            if self.cents.abs() <= self.tolerance {
                // Within tolerance: zone width is derived from tolerance
                // (see `zone_half_width`), not a fixed character count, so
                // it agrees with the tick marks at any tolerance or width.
                let zone_half_width = Self::zone_half_width(self.tolerance, max_cents, half_width);
                let half_zone = zone_half_width.round() as u16;
                let start_x = center_x.saturating_sub(half_zone).max(area.x);
                let end_x = (center_x + half_zone + 1).min(area.x + area.width);

                // Sub-tolerance drift: map the smoothed position (issue
                // #32) linearly across the zone so fine settling is still
                // visible instead of the needle teleporting to a static,
                // information-free block.
                let needle_offset = Self::sub_tolerance_offset(
                    self.smoothed_cents,
                    self.tolerance,
                    zone_half_width,
                );
                let needle_x = (center_x as i32 + needle_offset.round() as i32)
                    .clamp(area.x as i32, area.x as i32 + area.width as i32 - 1)
                    as u16;
                let needle_style = Theme::accent().add_modifier(Modifier::BOLD);
                // Lock-flash (issue #32): a brief inverted pop the frame the
                // reading first settles into the zone, distinguishing "just
                // locked" from "sitting in tune" without a lasting change.
                let zone_style = if self.flashing {
                    style.add_modifier(Modifier::REVERSED | Modifier::BOLD)
                } else {
                    style
                };

                for y in indicator_start..indicator_end {
                    for x in start_x..end_x {
                        if x == needle_x {
                            // Brighter cell within the zone: shows exactly
                            // where inside the tolerance the reading sits.
                            buf.set_string(
                                x,
                                y,
                                BoxChars::THICK_VERTICAL.to_string(),
                                needle_style,
                            );
                        } else {
                            buf.set_string(x, y, "█", zone_style);
                        }
                    }
                }
            } else {
                // Outside tolerance: narrow indicator at logarithmic
                // position, tracking the smoothed value (issue #32).
                let clamped_cents = self.smoothed_cents.clamp(-max_cents, max_cents);
                let x_offset = Self::log_position(
                    clamped_cents,
                    max_cents,
                    half_width,
                    Self::LOG_SCALE_PIVOT_CENTS,
                );
                let indicator_x = (center_x as f32 + x_offset) as u16;

                // Narrow indicator (1-2 chars) when out of tune
                for y in indicator_start..indicator_end {
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

    // -- in-tune zone width, derived from tolerance (issue #33) --

    #[test]
    fn test_zone_half_width_matches_hand_computed_value() {
        // Same log curve as `log_position`, pivoted at 1 cent instead of at
        // `tolerance` (pivoting on tolerance itself always maps tolerance to
        // offset 0, which can't yield a non-zero zone width).
        let expected = (5f32).ln() / (500f32).ln() * 50.0;
        let actual = Meter::zone_half_width(5.0, 500.0, 50.0);
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn test_zone_half_width_grows_with_tolerance() {
        let narrow = Meter::zone_half_width(5.0, 500.0, 50.0);
        let wide = Meter::zone_half_width(10.0, 500.0, 50.0);
        assert!(
            wide > narrow,
            "a looser tolerance must draw a wider zone: {wide} should be > {narrow}"
        );
    }

    #[test]
    fn test_zone_half_width_is_not_the_old_hardcoded_constant() {
        // The bug this fixes: a fixed 7-wide block regardless of tolerance
        // or terminal width. At this width/tolerance the derived value must
        // differ from the old hardcoded half (3).
        let actual = Meter::zone_half_width(5.0, 500.0, 50.0);
        assert!(
            (actual - 3.0).abs() > 0.5,
            "zone width must be derived, not the old fixed 7-wide block"
        );
    }

    #[test]
    fn test_zone_half_width_reaches_full_width_at_max_cents() {
        let actual = Meter::zone_half_width(500.0, 500.0, 50.0);
        assert!((actual - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_zone_half_width_zero_at_or_below_pivot() {
        assert_eq!(Meter::zone_half_width(1.0, 500.0, 50.0), 0.0);
        assert_eq!(Meter::zone_half_width(0.5, 500.0, 50.0), 0.0);
    }

    #[test]
    fn test_zone_half_width_scales_with_terminal_width() {
        let narrow_term = Meter::zone_half_width(5.0, 500.0, 20.0);
        let wide_term = Meter::zone_half_width(5.0, 500.0, 60.0);
        assert!(wide_term > narrow_term);
    }

    // -- sub-tolerance drift: fine movement inside the zone (issue #33) --

    #[test]
    fn test_sub_tolerance_offset_at_center_is_zero() {
        assert_eq!(Meter::sub_tolerance_offset(0.0, 5.0, 10.0), 0.0);
    }

    #[test]
    fn test_sub_tolerance_offset_at_tolerance_boundary_reaches_zone_edge() {
        // This is what makes the zone and the log-scale ticks agree: at
        // cents == tolerance, sub-tolerance drift lands exactly on
        // `zone_half_width`, the same x-position where the log curve
        // (pivoted the same way) starts for values just past tolerance.
        assert_eq!(Meter::sub_tolerance_offset(5.0, 5.0, 10.0), 10.0);
        assert_eq!(Meter::sub_tolerance_offset(-5.0, 5.0, 10.0), -10.0);
    }

    #[test]
    fn test_sub_tolerance_offset_is_linear() {
        assert_eq!(Meter::sub_tolerance_offset(2.5, 5.0, 10.0), 5.0);
        assert_eq!(Meter::sub_tolerance_offset(-1.25, 5.0, 10.0), -2.5);
    }

    #[test]
    fn test_sub_tolerance_offset_clamps_beyond_tolerance() {
        // Defensive: callers only pass cents within tolerance, but the
        // mapping must not overshoot the zone if they don't.
        assert_eq!(Meter::sub_tolerance_offset(8.0, 5.0, 10.0), 10.0);
        assert_eq!(Meter::sub_tolerance_offset(-8.0, 5.0, 10.0), -10.0);
    }

    #[test]
    fn test_sub_tolerance_offset_zero_tolerance_does_not_panic() {
        assert_eq!(Meter::sub_tolerance_offset(1.0, 0.0, 10.0), 0.0);
    }

    // -- rendering: zone width and needle placement actually drawn --

    fn meter_row_symbols(area: Rect, buf: &Buffer, y: u16) -> Vec<String> {
        (area.x..area.x + area.width)
            .map(|x| buf[(x, y)].symbol().to_string())
            .collect()
    }

    #[test]
    fn test_render_zone_width_matches_computed_value() {
        let area = Rect::new(0, 0, 80, 8);
        let mut buf = Buffer::empty(area);
        let meter = Meter::new(0.0).tolerance(5.0);
        meter.render(area, &mut buf);

        let half_width = (area.width / 2 - 1) as f32;
        let expected_half = Meter::zone_half_width(5.0, 500.0, half_width).round() as u16;
        let center_x = area.x + area.width / 2;
        let y = area.y + 2; // meter_y_start

        let row = meter_row_symbols(area, &buf, y);
        let block = "█".to_string();
        let needle = BoxChars::THICK_VERTICAL.to_string();

        for x in (center_x - expected_half)..=(center_x + expected_half) {
            let symbol = &row[(x - area.x) as usize];
            assert!(
                *symbol == block || *symbol == needle,
                "cell at {x} should be inside the computed in-tune zone, got {symbol:?}"
            );
        }
        assert_ne!(
            row[(center_x - expected_half - 1 - area.x) as usize],
            block,
            "cell just outside the computed zone must not be filled"
        );
    }

    #[test]
    fn test_render_needle_shows_sub_tolerance_drift() {
        let area = Rect::new(0, 0, 80, 8);
        let mut buf = Buffer::empty(area);
        // Half of tolerance: within the in-tune zone, but off-center.
        let meter = Meter::new(2.5).tolerance(5.0);
        meter.render(area, &mut buf);

        let half_width = (area.width / 2 - 1) as f32;
        let zone_half = Meter::zone_half_width(5.0, 500.0, half_width);
        let needle_offset = Meter::sub_tolerance_offset(2.5, 5.0, zone_half);
        let center_x = area.x + area.width / 2;
        let needle_x = (center_x as f32 + needle_offset).round() as u16;
        let y = area.y + 2;

        assert_ne!(
            needle_x, center_x,
            "test setup should pick a drifting reading"
        );
        assert_eq!(
            buf[(needle_x, y)].symbol(),
            BoxChars::THICK_VERTICAL.to_string(),
            "needle must be visible at its sub-tolerance position"
        );
    }

    // -- needle smoothing, trail, and lock-flash rendering (issue #32) --

    #[test]
    fn test_smoothed_position_used_instead_of_raw_cents() {
        // Raw cents (30, out of tolerance) would plot far from center; the
        // smoothed value (0, mid-interpolation) must be what's drawn.
        let area = Rect::new(0, 0, 80, 8);
        let mut buf = Buffer::empty(area);
        let meter = Meter::new(30.0).tolerance(5.0).smoothed(0.0);
        meter.render(area, &mut buf);

        let center_x = area.x + area.width / 2;
        let y = area.y + 2;
        assert_eq!(
            buf[(center_x, y)].symbol(),
            "█",
            "indicator must track the smoothed position, not the raw reading"
        );
    }

    #[test]
    fn test_default_smoothed_matches_raw_cents_when_unset() {
        // Backward compatibility: callers that never call `.smoothed()` get
        // exactly the old snap-to-reading behavior.
        let area = Rect::new(0, 0, 80, 8);
        let mut buf_default = Buffer::empty(area);
        let mut buf_explicit = Buffer::empty(area);

        Meter::new(42.0)
            .tolerance(5.0)
            .render(area, &mut buf_default);
        Meter::new(42.0)
            .tolerance(5.0)
            .smoothed(42.0)
            .render(area, &mut buf_explicit);

        for y in 0..area.height {
            for x in 0..area.width {
                assert_eq!(
                    buf_default[(x, y)].symbol(),
                    buf_explicit[(x, y)].symbol(),
                    "mismatch at ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn test_empty_trail_does_not_shrink_the_indicator_rows() {
        // No trail history supplied: every meter row must still go to the
        // main indicator, exactly like before the trail existed.
        let area = Rect::new(0, 0, 80, 8);
        let mut buf_no_trail = Buffer::empty(area);
        let mut buf_baseline = Buffer::empty(area);

        Meter::new(0.0)
            .tolerance(5.0)
            .trail([])
            .render(area, &mut buf_no_trail);
        Meter::new(0.0)
            .tolerance(5.0)
            .render(area, &mut buf_baseline);

        for y in 0..area.height {
            for x in 0..area.width {
                assert_eq!(buf_no_trail[(x, y)].symbol(), buf_baseline[(x, y)].symbol());
            }
        }
    }

    #[test]
    fn test_trail_marks_are_drawn_on_the_reserved_row() {
        let area = Rect::new(0, 0, 80, 8);
        let mut buf = Buffer::empty(area);
        // Two trail samples away from center so they land at distinguishable columns.
        let meter = Meter::new(0.0)
            .tolerance(5.0)
            .trail([(-100.0, 0.5), (100.0, 1.0)]);
        meter.render(area, &mut buf);

        let trail_y = area.y + 2; // meter_y_start, reserved when trail is non-empty
        let has_partial_block = (area.x..area.x + area.width).any(|x| {
            BoxChars::BLOCKS.contains(&buf[(x, trail_y)].symbol().chars().next().unwrap())
        });
        assert!(
            has_partial_block,
            "expected a trail mark on the reserved row"
        );
    }

    #[test]
    fn test_flashing_applies_reversed_modifier_to_the_zone() {
        let area = Rect::new(0, 0, 80, 8);
        let mut buf = Buffer::empty(area);
        let meter = Meter::new(0.0).tolerance(5.0).flashing(true);
        meter.render(area, &mut buf);

        let center_x = area.x + area.width / 2;
        // Just past center (not the needle column itself) to land on a
        // plain zone-fill cell rather than the needle's own style.
        let x = center_x + 1;
        let y = area.y + 2;
        assert!(
            buf[(x, y)]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED),
            "flashing must invert the zone fill"
        );
    }

    #[test]
    fn test_render_smoke_at_small_sizes_does_not_panic() {
        for (w, h) in [(20, 5), (40, 8), (80, 8), (1, 1), (0, 0)] {
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(area);
            Meter::new(3.0)
                .tolerance(5.0)
                .smoothed(1.5)
                .trail([(0.0, 0.3), (1.0, 0.6), (1.5, 1.0)])
                .flashing(true)
                .render(area, &mut buf);
        }
    }
}
