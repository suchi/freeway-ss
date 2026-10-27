//! Sizing and geometry of the speedometer, independent of the OS so that it
//! is tested everywhere. The renderer only turns the numbers of this module
//! into Direct2D and DirectWrite calls.

use freeway_core::SpeedometerStyle;

/// Height in logical pixels (at 96 DPI) from which the speedometer has its
/// full size. This is below the height of any real full-screen display, so
/// the speedometer only shrinks in small windows such as the preview in the
/// screensaver settings (about 152 x 112 pixels).
const FULL_SIZE_HEIGHT: f32 = 540.0;

/// The speed to show for the actual speed, in whole km/h. The actual speed
/// drifts continuously, so it is rounded to the nearest km/h; an invalid
/// speed shows 0.
pub fn displayed_speed(speed_kmh: f32) -> u32 {
    if speed_kmh.is_finite() && speed_kmh > 0.0 {
        speed_kmh.round() as u32
    } else {
        0
    }
}

/// Returns the factor that the speedometer's sizes (defined in pixels at a
/// scale of 1) are multiplied with, for a window of `height` physical pixels.
///
/// The factor is `pixel_scale`, so that the text keeps its size on screen at
/// any DPI, but at most proportional to the height of windows lower than
/// `FULL_SIZE_HEIGHT` logical pixels, so that the text never fills a small
/// window. Returns 0 for invalid input, which draws nothing.
pub fn scale(height: f32, pixel_scale: f32) -> f32 {
    if !(height.is_finite() && height > 0.0 && pixel_scale.is_finite() && pixel_scale > 0.0) {
        return 0.0;
    }
    pixel_scale.min(height / FULL_SIZE_HEIGHT)
}

// Everything below is in pixels at a scale of 1 (see `scale`), unless it is
// named otherwise.

/// Distance from the left and bottom edge of the window to the speedometer.
pub const MARGIN: f32 = 28.0;

/// Angle of the start of a gauge, in degrees clockwise from the 3 o'clock
/// direction as seen on the screen (8 o'clock). The scale runs clockwise
/// through the top to `GAUGE_START_DEG + GAUGE_SWEEP_DEG` (4 o'clock).
pub const GAUGE_START_DEG: f32 = 150.0;
pub const GAUGE_SWEEP_DEG: f32 = 240.0;

/// The speeds at the ends of the speed gauge.
pub const SPEED_GAUGE_MAX_KMH: f32 = 180.0;
/// The engine speeds at the end of the tachometer, and from where its scale is red.
pub const TACH_MAX_RPM: f32 = 8000.0;
pub const TACH_RED_RPM: f32 = 6000.0;
/// The engine speed per km/h of an automatic gearbox in its top gear (about
/// 2,000 rpm at 100 km/h).
const RPM_PER_KMH: f32 = 20.0;

/// What a gauge looks like, at a scale of 1. The insets are measured from
/// the radius of the arc towards the center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GaugeSpec {
    pub radius: f32,
    /// Width of the arc.
    pub stroke: f32,
    /// Where the ticks start (their outer ends).
    pub tick_inset: f32,
    pub major_tick: f32,
    pub minor_tick: f32,
    /// Where the centers of the numbers of the scale are.
    pub label_inset: f32,
    pub label_size: f32,
    /// Where the needle starts and ends.
    pub needle_inner_inset: f32,
    pub needle_outer_inset: f32,
    pub needle_width: f32,
}

/// The gauge of the simple dashboard: an arc only.
pub const SIMPLE_GAUGE: GaugeSpec = GaugeSpec {
    radius: 90.0,
    stroke: 8.0,
    tick_inset: 0.0,
    major_tick: 0.0,
    minor_tick: 0.0,
    label_inset: 0.0,
    label_size: 0.0,
    needle_inner_inset: 0.0,
    needle_outer_inset: 0.0,
    needle_width: 0.0,
};
/// The speed gauge of the detailed dashboard.
pub const SPEED_GAUGE: GaugeSpec = GaugeSpec {
    radius: 88.0,
    stroke: 8.0,
    tick_inset: 10.0,
    major_tick: 11.0,
    minor_tick: 6.0,
    label_inset: 31.0,
    label_size: 10.0,
    needle_inner_inset: 24.0,
    needle_outer_inset: 3.0,
    needle_width: 4.0,
};
/// The tachometer of the detailed dashboard.
pub const TACH_GAUGE: GaugeSpec = GaugeSpec {
    radius: 54.0,
    stroke: 6.0,
    tick_inset: 8.0,
    major_tick: 7.0,
    minor_tick: 7.0,
    label_inset: 20.0,
    label_size: 9.0,
    needle_inner_inset: 16.0,
    needle_outer_inset: 2.0,
    needle_width: 3.0,
};

/// Size of the number in the middle of the speed gauge of each dashboard, and
/// of the unit under it.
pub const SIMPLE_NUMBER_SIZE: f32 = 44.0;
pub const DETAILED_NUMBER_SIZE: f32 = 36.0;
pub const GAUGE_UNIT_SIZE: f32 = 16.0;
/// Size of the caption in the middle of the tachometer.
pub const TACH_CAPTION_SIZE: f32 = 9.0;
/// Size of the texts of the row of information.
pub const INFO_TEXT_SIZE: f32 = 13.0;
/// Texts that would be smaller than this on the screen (in pixels) are not
/// drawn: in the preview of the settings they would only be noise.
pub const MIN_TEXT_PIXELS: f32 = 6.0;

/// Padding of the panel behind the detailed dashboard, the gap between its
/// two gauges, and the row of information under them.
const PANEL_PADDING: f32 = 14.0;
const GAUGE_GAP: f32 = 12.0;
const INFO_GAP: f32 = 8.0;
const INFO_HEIGHT: f32 = 20.0;
pub const PANEL_CORNER: f32 = 14.0;

/// The gear that is shown; the car is an automatic one that always drives.
pub const GEAR_TEXT: &str = "D";
/// The caption of the tachometer.
pub const TACH_CAPTION: &str = "x1000 rpm";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// A gauge placed in the window: its center and size in pixels, with `scale`
/// applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gauge {
    pub center: Point,
    pub spec: GaugeSpec,
    pub scale: f32,
}

impl Gauge {
    /// Radius of the arc in pixels.
    pub fn radius(&self) -> f32 {
        self.spec.radius * self.scale
    }

    /// Width of the arc in pixels.
    pub fn stroke(&self) -> f32 {
        self.spec.stroke * self.scale
    }

    /// The point `inset` (at a scale of 1) inside the arc, at the place on
    /// the scale that `fraction` (0 to 1, clamped) means.
    pub fn point_at(&self, inset: f32, fraction: f32) -> Point {
        let angle = gauge_angle(fraction).to_radians();
        let radius = self.radius() - inset * self.scale;
        Point {
            x: self.center.x + radius * angle.cos(),
            y: self.center.y + radius * angle.sin(),
        }
    }
}

/// The angle of the place that `fraction` (0 to 1) of a scale means, in
/// degrees as described at `GAUGE_START_DEG`. A fraction outside the range
/// stops at the nearer end, an invalid one gives the start.
pub fn gauge_angle(fraction: f32) -> f32 {
    GAUGE_START_DEG + GAUGE_SWEEP_DEG * clamp_fraction(fraction)
}

fn clamp_fraction(fraction: f32) -> f32 {
    if fraction.is_nan() {
        0.0
    } else {
        fraction.clamp(0.0, 1.0)
    }
}

/// Where a speed is on the speed gauge, from 0 to 1.
pub fn speed_fraction(speed_kmh: f32) -> f32 {
    clamp_fraction(speed_kmh / SPEED_GAUGE_MAX_KMH)
}

/// Where an engine speed is on the tachometer, from 0 to 1.
pub fn rpm_fraction(rpm: f32) -> f32 {
    clamp_fraction(rpm / TACH_MAX_RPM)
}

/// The engine speed at `speed_kmh`: proportional to the speed, as in a car
/// that drives in its top gear. The drift of the speed makes it sway.
pub fn engine_rpm(speed_kmh: f32) -> f32 {
    if speed_kmh.is_finite() && speed_kmh > 0.0 {
        (speed_kmh * RPM_PER_KMH).min(TACH_MAX_RPM)
    } else {
        0.0
    }
}

/// A tick of a scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tick {
    /// The number at the tick: km/h on the speed gauge, thousand rpm on the
    /// tachometer.
    pub value: u32,
    /// Place on the scale, from 0 to 1.
    pub fraction: f32,
    /// Long and with a number.
    pub major: bool,
}

/// The ticks of the speed gauge: one every 10 km/h, long and with a number
/// every 20 km/h.
pub fn speed_ticks() -> impl Iterator<Item = Tick> {
    (0..=(SPEED_GAUGE_MAX_KMH as u32 / 10)).map(|step| {
        let kmh = step * 10;
        Tick {
            value: kmh,
            fraction: kmh as f32 / SPEED_GAUGE_MAX_KMH,
            major: kmh % 20 == 0,
        }
    })
}

/// The ticks of the tachometer: one every 1,000 rpm, each with a number.
pub fn tach_ticks() -> impl Iterator<Item = Tick> {
    (0..=(TACH_MAX_RPM as u32 / 1000)).map(|thousand| Tick {
        value: thousand,
        fraction: thousand as f32 * 1000.0 / TACH_MAX_RPM,
        major: true,
    })
}

/// The time of day as `HH:MM`, from the minutes after midnight (wrapping
/// around the day). An invalid time shows `00:00`.
pub fn clock_text(minute_of_day: f32) -> String {
    let minute = if minute_of_day.is_finite() {
        minute_of_day.rem_euclid(1440.0) as u32 % 1440
    } else {
        0
    };
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// The distance driven as `TRIP 123.4 km`, from the distance in meters.
pub fn trip_text(meters: f64) -> String {
    let km = if meters.is_finite() && meters > 0.0 {
        meters / 1000.0
    } else {
        0.0
    };
    format!("TRIP {km:.1} km")
}

/// Whether text of `size` (at a scale of 1) is large enough to be drawn at `scale`.
pub fn text_visible(size: f32, scale: f32) -> bool {
    size * scale >= MIN_TEXT_PIXELS
}

/// Where the baselines of the number and of the unit under it are, below the
/// center of a gauge, for a number of `number_size` and a unit of
/// `unit_size` (all at a scale of 1). The digits and the unit, with the gap
/// between them, are centered on the gauge together.
pub fn number_baselines(number_size: f32, unit_size: f32) -> (f32, f32) {
    // The height of the digits and the gap between the two lines, as
    // fractions of the sizes of the fonts.
    const CAP_HEIGHT: f32 = 0.7;
    const GAP: f32 = 0.22;
    let total = number_size * (CAP_HEIGHT + GAP) + unit_size * CAP_HEIGHT;
    (-total / 2.0 + number_size * CAP_HEIGHT, total / 2.0)
}

/// Where the parts of a dashboard are in a window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dashboard {
    /// The dark panel behind everything; the simple dashboard has none.
    pub panel: Option<Rect>,
    pub speed: Gauge,
    pub tach: Option<Gauge>,
    /// The row for the time, the distance and the gear.
    pub info: Option<Rect>,
}

/// How far below the center of a gauge its arc reaches: the ends of the
/// arc are its lowest points.
fn below_center(spec: &GaugeSpec) -> f32 {
    spec.radius * GAUGE_START_DEG.to_radians().sin() + spec.stroke / 2.0
}

/// Distance from the center of a gauge to the edge of its arc, in any
/// direction but down.
fn around_center(spec: &GaugeSpec) -> f32 {
    spec.radius + spec.stroke / 2.0
}

/// Places the dashboard of `style` in the lower left corner of a window of
/// `width` x `height` pixels, with the `scale` from `scale`. Returns `None`
/// when `style` is not a dashboard, when the input is invalid, or when the
/// dashboard does not fit in the window (then nothing is drawn).
pub fn dashboard_layout(
    style: SpeedometerStyle,
    width: f32,
    height: f32,
    scale: f32,
) -> Option<Dashboard> {
    let valid = |value: f32| value.is_finite() && value > 0.0;
    if !(valid(width) && valid(height) && valid(scale)) {
        return None;
    }
    let left = MARGIN * scale;
    let bottom = height - MARGIN * scale;
    let gauge = |spec: GaugeSpec, x: f32, y: f32| Gauge {
        center: Point { x, y },
        spec,
        scale,
    };
    let dashboard = match style {
        SpeedometerStyle::SimpleDashboard => {
            let spec = SIMPLE_GAUGE;
            Dashboard {
                panel: None,
                speed: gauge(
                    spec,
                    left + around_center(&spec) * scale,
                    bottom - below_center(&spec) * scale,
                ),
                tach: None,
                info: None,
            }
        }
        SpeedometerStyle::DetailedDashboard => {
            let (speed, tach) = (SPEED_GAUGE, TACH_GAUGE);
            let padding = PANEL_PADDING * scale;
            let gauges_height = (around_center(&speed) + below_center(&speed)) * scale;
            let panel_width = padding * 2.0
                + (around_center(&speed) * 2.0 + GAUGE_GAP + around_center(&tach) * 2.0) * scale;
            let panel_height = padding * 2.0 + gauges_height + (INFO_GAP + INFO_HEIGHT) * scale;
            let panel = Rect {
                left,
                top: bottom - panel_height,
                right: left + panel_width,
                bottom,
            };
            let gauges_bottom = panel.top + padding + gauges_height;
            let info_top = gauges_bottom + INFO_GAP * scale;
            Dashboard {
                panel: Some(panel),
                speed: gauge(
                    speed,
                    left + padding + around_center(&speed) * scale,
                    panel.top + padding + around_center(&speed) * scale,
                ),
                // The tachometer is smaller; the bottoms of the two arcs are level.
                tach: Some(gauge(
                    tach,
                    left + padding
                        + (around_center(&speed) * 2.0 + GAUGE_GAP + around_center(&tach)) * scale,
                    gauges_bottom - below_center(&tach) * scale,
                )),
                info: Some(Rect {
                    left: left + padding,
                    top: info_top,
                    right: panel.right - padding,
                    bottom: info_top + INFO_HEIGHT * scale,
                }),
            }
        }
        SpeedometerStyle::None | SpeedometerStyle::Digits => return None,
    };
    let (right, top) = match dashboard.panel {
        Some(panel) => (panel.right, panel.top),
        None => (
            dashboard.speed.center.x + around_center(&dashboard.speed.spec) * scale,
            dashboard.speed.center.y - around_center(&dashboard.speed.spec) * scale,
        ),
    };
    (right <= width && top >= 0.0).then_some(dashboard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_speed_is_rounded_to_whole_kmh() {
        assert_eq!(displayed_speed(100.0), 100);
        assert_eq!(displayed_speed(99.6), 100);
        assert_eq!(displayed_speed(100.4), 100);
        assert_eq!(displayed_speed(87.5), 88);
        assert_eq!(displayed_speed(160.0), 160);
    }

    #[test]
    fn an_invalid_speed_shows_zero() {
        for speed in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 0.2] {
            assert_eq!(displayed_speed(speed), 0, "{speed}");
        }
    }

    #[test]
    fn full_screens_keep_the_size_of_their_dpi() {
        // 100%, 125%, 150% and 200% on common display heights.
        for (height, pixel_scale) in [
            (768.0, 1.0),
            (1080.0, 1.0),
            (1080.0, 1.25),
            (1440.0, 1.5),
            (2160.0, 1.5),
            (2160.0, 2.0),
        ] {
            assert_eq!(
                scale(height, pixel_scale),
                pixel_scale,
                "{height} at {pixel_scale}"
            );
        }
    }

    #[test]
    fn the_preview_is_proportional_to_its_height() {
        let small = scale(112.0, 1.0);
        assert!((small - 112.0 / 540.0).abs() < 1e-6);
        // The number (56 px at scale 1) stays well below the window height.
        assert!(56.0 * small < 112.0 * 0.15);
        // Twice the height, twice the size, as long as the window is small.
        assert!((scale(224.0, 1.0) - 2.0 * small).abs() < 1e-6);
    }

    #[test]
    fn a_high_dpi_preview_scales_with_its_height_too() {
        let small = scale(168.0, 1.5);
        assert!((small - 168.0 / 540.0).abs() < 1e-6);
    }

    #[test]
    fn the_size_never_exceeds_the_dpi_scale() {
        for height in [100.0, 540.0, 1000.0, 5000.0] {
            assert!(scale(height, 1.5) <= 1.5);
        }
    }

    #[test]
    fn invalid_input_gives_zero() {
        for (height, pixel_scale) in [
            (0.0, 1.0),
            (-5.0, 1.0),
            (f32::NAN, 1.0),
            (1080.0, 0.0),
            (1080.0, f32::NAN),
            (f32::INFINITY, 1.0),
        ] {
            assert_eq!(scale(height, pixel_scale), 0.0);
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn the_gauge_starts_at_zero_and_ends_at_the_maximum() {
        assert!(close(gauge_angle(speed_fraction(0.0)), GAUGE_START_DEG));
        assert!(close(
            gauge_angle(speed_fraction(SPEED_GAUGE_MAX_KMH)),
            GAUGE_START_DEG + GAUGE_SWEEP_DEG
        ));
        // 90 km/h is half way.
        assert!(close(
            gauge_angle(speed_fraction(90.0)),
            GAUGE_START_DEG + GAUGE_SWEEP_DEG / 2.0
        ));
    }

    #[test]
    fn the_gauge_angle_is_proportional_to_the_speed() {
        let angle = |kmh: f32| gauge_angle(speed_fraction(kmh));
        for kmh in [10.0, 45.0, 100.0, 160.0] {
            let expected = GAUGE_START_DEG + GAUGE_SWEEP_DEG * kmh / SPEED_GAUGE_MAX_KMH;
            assert!(close(angle(kmh), expected), "{kmh}");
        }
        assert!(close(
            angle(100.0) - angle(50.0),
            angle(150.0) - angle(100.0)
        ));
    }

    #[test]
    fn the_gauge_stops_at_its_ends() {
        let end = GAUGE_START_DEG + GAUGE_SWEEP_DEG;
        for kmh in [-50.0, -0.1] {
            assert!(close(gauge_angle(speed_fraction(kmh)), GAUGE_START_DEG));
        }
        for kmh in [180.1, 200.0, 1000.0, f32::INFINITY] {
            assert!(close(gauge_angle(speed_fraction(kmh)), end), "{kmh}");
        }
        assert!(close(
            gauge_angle(speed_fraction(f32::NAN)),
            GAUGE_START_DEG
        ));
        assert!(close(gauge_angle(f32::NEG_INFINITY), GAUGE_START_DEG));
        assert!(close(gauge_angle(f32::NAN), GAUGE_START_DEG));
    }

    #[test]
    fn the_arc_runs_clockwise_from_the_lower_left_to_the_lower_right() {
        let gauge = Gauge {
            center: Point { x: 100.0, y: 100.0 },
            spec: SIMPLE_GAUGE,
            scale: 1.0,
        };
        let start = gauge.point_at(0.0, 0.0);
        let middle = gauge.point_at(0.0, 0.5);
        let end = gauge.point_at(0.0, 1.0);
        // The start is left of and below the center, the end right of it and
        // as low; the middle is at the top (y grows downwards).
        assert!(start.x < 100.0 && start.y > 100.0);
        assert!(end.x > 100.0 && close(end.y, start.y));
        assert!(close(start.x - 100.0, 100.0 - end.x));
        assert!(close(middle.x, 100.0) && close(middle.y, 100.0 - SIMPLE_GAUGE.radius));
        // An inset moves the point towards the center, and the scale applies.
        let inside = gauge.point_at(10.0, 0.5);
        assert!(close(inside.y, 100.0 - SIMPLE_GAUGE.radius + 10.0));
        let doubled = Gauge {
            scale: 2.0,
            ..gauge
        };
        assert!(close(
            doubled.point_at(10.0, 0.5).y,
            100.0 - 2.0 * SIMPLE_GAUGE.radius + 20.0
        ));
    }

    #[test]
    fn the_speed_gauge_has_a_tick_every_10_kmh_and_a_number_every_20() {
        let ticks: Vec<Tick> = speed_ticks().collect();
        assert_eq!(ticks.len(), 19);
        let major: Vec<u32> = ticks.iter().filter(|t| t.major).map(|t| t.value).collect();
        assert_eq!(major, [0, 20, 40, 60, 80, 100, 120, 140, 160, 180]);
        let minor: Vec<u32> = ticks.iter().filter(|t| !t.major).map(|t| t.value).collect();
        assert_eq!(minor, [10, 30, 50, 70, 90, 110, 130, 150, 170]);
        for tick in &ticks {
            let angle = gauge_angle(tick.fraction);
            let expected = GAUGE_START_DEG + GAUGE_SWEEP_DEG * tick.value as f32 / 180.0;
            assert!(close(angle, expected), "{tick:?}");
        }
        assert!(close(ticks[0].fraction, 0.0));
        assert!(close(ticks[18].fraction, 1.0));
    }

    #[test]
    fn the_tachometer_has_a_number_every_1000_rpm() {
        let ticks: Vec<Tick> = tach_ticks().collect();
        assert_eq!(ticks.len(), 9);
        assert!(ticks.iter().all(|t| t.major));
        assert_eq!(
            ticks.iter().map(|t| t.value).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert!(close(ticks[6].fraction, TACH_RED_RPM / TACH_MAX_RPM));
        assert!(close(ticks[8].fraction, 1.0));
    }

    #[test]
    fn the_engine_runs_at_about_2000_rpm_at_100_kmh() {
        assert!((engine_rpm(100.0) - 2000.0).abs() < 100.0);
    }

    #[test]
    fn the_engine_speed_is_proportional_to_the_speed() {
        assert_eq!(engine_rpm(0.0), 0.0);
        assert!(close(engine_rpm(60.0) * 2.0, engine_rpm(120.0)));
        assert!(close(engine_rpm(80.0) * 2.0, engine_rpm(160.0)));
        // Within the speeds of the screensaver (up to 180 km/h with the
        // drift) it stays well below the red part of the tachometer.
        assert!(engine_rpm(180.0) < TACH_RED_RPM);
    }

    #[test]
    fn the_engine_speed_stays_within_the_tachometer() {
        for kmh in [-10.0, 0.0, 1.0, 400.0, 1e9, f32::NAN, f32::INFINITY] {
            let rpm = engine_rpm(kmh);
            assert!((0.0..=TACH_MAX_RPM).contains(&rpm), "{kmh}");
            let fraction = rpm_fraction(rpm);
            assert!((0.0..=1.0).contains(&fraction), "{kmh}");
        }
        assert!(close(rpm_fraction(4000.0), 0.5));
    }

    #[test]
    fn the_clock_shows_hours_and_minutes() {
        assert_eq!(clock_text(0.0), "00:00");
        assert_eq!(clock_text(59.9), "00:59");
        assert_eq!(clock_text(60.0), "01:00");
        assert_eq!(clock_text(9.0 * 60.0 + 5.0), "09:05");
        assert_eq!(clock_text(23.0 * 60.0 + 59.0), "23:59");
        assert_eq!(clock_text(1439.99), "23:59");
    }

    #[test]
    fn the_clock_wraps_around_the_day_and_survives_invalid_times() {
        assert_eq!(clock_text(1440.0), "00:00");
        assert_eq!(clock_text(1440.0 + 61.0), "01:01");
        assert_eq!(clock_text(-1.0), "23:59");
        for time in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(clock_text(time), "00:00");
        }
        // Every minute of the day has the format `HH:MM`.
        for minute in 0..1440 {
            let text = clock_text(minute as f32);
            let (hour, min) = text.split_once(':').unwrap();
            assert_eq!(text.len(), 5, "{text}");
            assert_eq!(hour.parse::<u32>().unwrap(), minute / 60);
            assert_eq!(min.parse::<u32>().unwrap(), minute % 60);
        }
    }

    #[test]
    fn the_trip_shows_kilometers_with_one_decimal() {
        assert_eq!(trip_text(0.0), "TRIP 0.0 km");
        assert_eq!(trip_text(1234.0), "TRIP 1.2 km");
        assert_eq!(trip_text(123_400.0), "TRIP 123.4 km");
        assert_eq!(trip_text(950.0), "TRIP 0.9 km");
        assert_eq!(trip_text(960.0), "TRIP 1.0 km");
        for meters in [-5.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(trip_text(meters), "TRIP 0.0 km", "{meters}");
        }
    }

    /// Windows heights with their DPI scale, and the size of the preview.
    const WINDOWS: [(f32, f32, f32); 8] = [
        (1366.0, 768.0, 1.0),
        (1920.0, 1080.0, 1.0),
        (1920.0, 1080.0, 1.25),
        (2560.0, 1440.0, 1.5),
        (3840.0, 2160.0, 1.5),
        (3840.0, 2160.0, 2.0),
        // The preview in the settings, at 100% and 150%.
        (152.0, 112.0, 1.0),
        (228.0, 168.0, 1.5),
    ];

    #[test]
    fn the_dashboards_fit_in_the_windows() {
        for style in [
            SpeedometerStyle::SimpleDashboard,
            SpeedometerStyle::DetailedDashboard,
        ] {
            for (width, height, pixel_scale) in WINDOWS {
                let scale = scale(height, pixel_scale);
                let dashboard = dashboard_layout(style, width, height, scale)
                    .unwrap_or_else(|| panic!("{style:?} {width}x{height}"));
                let bounds = dashboard.panel.unwrap_or_else(|| {
                    let gauge = dashboard.speed;
                    let reach = (
                        gauge.radius() + gauge.stroke() / 2.0,
                        below_center(&gauge.spec) * scale,
                    );
                    Rect {
                        left: gauge.center.x - reach.0,
                        top: gauge.center.y - reach.0,
                        right: gauge.center.x + reach.0,
                        bottom: gauge.center.y + reach.1,
                    }
                });
                let label = format!("{style:?} {width}x{height} at {pixel_scale}");
                assert!(bounds.left >= 0.0 && bounds.top >= 0.0, "{label}");
                assert!(bounds.right <= width && bounds.bottom <= height, "{label}");
                // The corner of the window is kept free.
                assert!(close(bounds.left, MARGIN * scale), "{label}");
                assert!(close(bounds.bottom, height - MARGIN * scale), "{label}");
                // It never takes much of the height, not even in the preview.
                assert!(bounds.bottom - bounds.top < height * 0.4, "{label}");
            }
        }
    }

    #[test]
    fn the_parts_of_the_detailed_dashboard_lie_inside_its_panel() {
        for (width, height, pixel_scale) in WINDOWS {
            let scale = scale(height, pixel_scale);
            let dashboard =
                dashboard_layout(SpeedometerStyle::DetailedDashboard, width, height, scale)
                    .unwrap();
            let panel = dashboard.panel.unwrap();
            let tach = dashboard.tach.unwrap();
            let info = dashboard.info.unwrap();
            for gauge in [dashboard.speed, tach] {
                let reach = gauge.radius() + gauge.stroke() / 2.0;
                let below = below_center(&gauge.spec) * scale;
                assert!(gauge.center.x - reach >= panel.left);
                assert!(gauge.center.x + reach <= panel.right);
                assert!(gauge.center.y - reach >= panel.top);
                // The arcs end above the row of information.
                assert!(gauge.center.y + below <= info.top + 1e-3);
            }
            // The two gauges do not overlap, and their bottoms are level.
            let speed_reach = dashboard.speed.radius() + dashboard.speed.stroke() / 2.0;
            let tach_reach = tach.radius() + tach.stroke() / 2.0;
            assert!(tach.center.x - tach_reach > dashboard.speed.center.x + speed_reach);
            let bottom = |gauge: &Gauge| gauge.center.y + below_center(&gauge.spec) * scale;
            assert!(close(bottom(&dashboard.speed), bottom(&tach)));
            assert!(info.left >= panel.left && info.right <= panel.right);
            assert!(info.bottom <= panel.bottom && info.top > bottom(&tach));
        }
    }

    #[test]
    fn the_simple_dashboard_has_one_gauge_only() {
        let dashboard =
            dashboard_layout(SpeedometerStyle::SimpleDashboard, 1920.0, 1080.0, 1.0).unwrap();
        assert!(dashboard.panel.is_none() && dashboard.tach.is_none() && dashboard.info.is_none());
        assert_eq!(dashboard.speed.spec, SIMPLE_GAUGE);
    }

    #[test]
    fn only_the_dashboards_have_a_layout() {
        for style in [SpeedometerStyle::None, SpeedometerStyle::Digits] {
            assert_eq!(dashboard_layout(style, 1920.0, 1080.0, 1.0), None);
        }
    }

    #[test]
    fn a_dashboard_that_does_not_fit_is_not_laid_out() {
        // A window that is too narrow or too low, at the full size.
        for (width, height) in [(200.0, 1080.0), (1920.0, 150.0)] {
            for style in [
                SpeedometerStyle::SimpleDashboard,
                SpeedometerStyle::DetailedDashboard,
            ] {
                assert_eq!(dashboard_layout(style, width, height, 1.0), None);
            }
        }
    }

    #[test]
    fn an_invalid_window_or_scale_has_no_layout() {
        for (width, height, scale) in [
            (0.0, 1080.0, 1.0),
            (1920.0, 0.0, 1.0),
            (1920.0, 1080.0, 0.0),
            (f32::NAN, 1080.0, 1.0),
            (1920.0, f32::NAN, 1.0),
            (1920.0, 1080.0, f32::NAN),
            (1920.0, 1080.0, f32::INFINITY),
            (-1.0, 1080.0, 1.0),
        ] {
            for style in SpeedometerStyle::ALL {
                assert_eq!(dashboard_layout(style, width, height, scale), None);
            }
        }
    }

    #[test]
    fn the_number_is_above_its_unit_and_both_are_inside_the_gauge() {
        for (number_size, unit_size, gauge) in [
            (SIMPLE_NUMBER_SIZE, GAUGE_UNIT_SIZE, SIMPLE_GAUGE),
            (DETAILED_NUMBER_SIZE, GAUGE_UNIT_SIZE, SPEED_GAUGE),
        ] {
            let (number, unit) = number_baselines(number_size, unit_size);
            assert!(unit > number);
            // The top of the digits is above the center, the unit is above
            // the ends of the arc.
            assert!(number - number_size * 0.7 < 0.0);
            assert!(unit < below_center(&gauge) - gauge.stroke);
        }
    }

    #[test]
    fn small_text_is_not_drawn() {
        assert!(text_visible(13.0, 1.0));
        assert!(text_visible(9.0, 1.0));
        assert!(!text_visible(13.0, 0.2));
        assert!(!text_visible(16.0, 112.0 / 540.0));
        assert!(text_visible(SIMPLE_NUMBER_SIZE, 112.0 / 540.0));
    }
}
