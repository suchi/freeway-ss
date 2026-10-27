//! The dashboard speedometers (simple and detailed), drawn with Direct2D and
//! DirectWrite. Where everything is comes from `crate::speedometer`; this
//! module only turns it into drawing calls.
//!
//! What does not depend on the speed (arcs, ticks, numbers of the scales) is
//! made once per style and scale and kept; the value arcs, the needles and the
//! changing texts are made for each frame.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use freeway_core::{Scene, SpeedometerStyle, Viewport};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL, D2D1_DRAW_TEXT_OPTIONS_NONE,
    D2D1_ROUNDED_RECT, D2D1_SWEEP_DIRECTION_CLOCKWISE, ID2D1DeviceContext, ID2D1Factory1,
    ID2D1PathGeometry1, ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, IDWriteFactory, IDWriteTextLayout,
};
use windows::core::{Error, Result};
use windows_numerics::{Matrix3x2, Vector2};

use super::renderer::{Face, NUMBER_FACE, SPEEDOMETER_COLOR as AMBER, UNIT_FACE, layout};
use crate::speedometer::{
    self, Gauge, GaugeSpec, Point, Rect, SIMPLE_GAUGE, SPEED_GAUGE, TACH_GAUGE, Tick,
};

/// Near white, for the numbers of the scales and the row of information.
const WHITE: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.94,
    g: 0.94,
    b: 0.94,
    a: 1.0,
};
/// Red for the end of the tachometer (#ff3b30).
const RED: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 1.0,
    g: 59.0 / 255.0,
    b: 48.0 / 255.0,
    a: 1.0,
};
const BLACK: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};

/// Opacities.
const PANEL_ALPHA: f32 = 0.45;
/// The arc behind the value, and the red end of the tachometer.
const TRACK_ALPHA: f32 = 0.25;
const RED_TRACK_ALPHA: f32 = 0.55;
const VALUE_ALPHA: f32 = 0.9;
const MAJOR_TICK_ALPHA: f32 = 0.85;
const MINOR_TICK_ALPHA: f32 = 0.5;
const LABEL_ALPHA: f32 = 0.7;
const NEEDLE_ALPHA: f32 = 0.95;
const NUMBER_ALPHA: f32 = 0.9;
const UNIT_ALPHA: f32 = 0.6;
const CAPTION_ALPHA: f32 = 0.5;
const INFO_ALPHA: f32 = 0.7;

/// Widths of the ticks, at a scale of 1.
const MAJOR_TICK_WIDTH: f32 = 2.0;
const MINOR_TICK_WIDTH: f32 = 1.5;

/// Digits of equal width, so that the clock and the distance do not shift.
const INFO_FACE: Face = Face {
    weight: DWRITE_FONT_WEIGHT_NORMAL,
    stretch: DWRITE_FONT_STRETCH_NORMAL,
    tabular_figures: true,
};

/// Fraction of the size of a font from the baseline up to the middle of the
/// digits, to center a number on a point.
const DIGITS_MIDDLE: f32 = 0.35;

/// What drawing needs from the renderer.
pub struct Devices<'a> {
    pub context: &'a ID2D1DeviceContext,
    pub factory: &'a ID2D1Factory1,
    pub dwrite: &'a IDWriteFactory,
    /// A stroke style with round caps.
    pub stroke_style: &'a ID2D1StrokeStyle,
}

/// The dashboard speedometers of a renderer.
pub struct Dashboards {
    brush: ID2D1SolidColorBrush,
    /// By (style, scale): the parts that do not change with the speed.
    parts: HashMap<(u32, u32), Parts>,
    texts: Texts,
}

/// The texts that change while the screensaver runs.
#[derive(Default)]
struct Texts {
    speed_number: TextCache,
    clock: TextCache,
    trip: TextCache,
}

impl Dashboards {
    pub fn new(context: &ID2D1DeviceContext) -> Result<Self> {
        Ok(Self {
            brush: unsafe { context.CreateSolidColorBrush(&AMBER, None)? },
            parts: HashMap::new(),
            texts: Texts::default(),
        })
    }

    /// Draws the dashboard of `style` over the scene with normal blending.
    /// Does nothing for a style without a dashboard, and when the dashboard
    /// does not fit in the window. Errors are returned for the caller to
    /// report after `EndDraw`.
    pub fn draw(
        &mut self,
        devices: &Devices,
        style: SpeedometerStyle,
        scene: &Scene,
        viewport: Viewport,
    ) -> Result<()> {
        let scale = speedometer::scale(viewport.height, viewport.pixel_scale);
        let Some(dashboard) =
            speedometer::dashboard_layout(style, viewport.width, viewport.height, scale)
        else {
            return Ok(());
        };
        let key = (style.to_raw(), scale.to_bits());
        // Why: one entry per style and scale, and a screensaver has one style
        // and a few scales (one per monitor), so this stays small.
        let parts = match self.parts.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(Parts::new(devices, style, scale)?),
        };

        let speed = scene.speed_kmh();
        let detailed = style == SpeedometerStyle::DetailedDashboard;
        let number_size = if detailed {
            speedometer::DETAILED_NUMBER_SIZE
        } else {
            speedometer::SIMPLE_NUMBER_SIZE
        };

        unsafe {
            devices.context.SetTransform(&Matrix3x2::identity());
        }
        if let Some(panel) = dashboard.panel {
            fill_panel(devices, &self.brush, panel, scale);
        }

        // The speed gauge, with its number and unit in the middle.
        let speed_fraction = speedometer::speed_fraction(speed);
        draw_gauge(
            devices,
            &self.brush,
            &dashboard.speed,
            &parts.speed,
            speed_fraction,
            detailed,
        )?;
        let center = dashboard.speed.center;
        if speedometer::text_visible(number_size, scale) {
            let text = self.texts.speed_number.get(
                devices.dwrite,
                &speedometer::displayed_speed(speed).to_string(),
                number_size * scale,
                NUMBER_FACE,
            )?;
            let (number_dy, _) =
                speedometer::number_baselines(number_size, speedometer::GAUGE_UNIT_SIZE);
            draw_text(
                devices,
                &self.brush,
                text,
                Vector2 {
                    X: center.x - text.width / 2.0,
                    Y: center.y + number_dy * scale - text.baseline,
                },
                AMBER,
                NUMBER_ALPHA,
            );
        }
        if let Some(unit) = &parts.unit {
            let (_, unit_dy) =
                speedometer::number_baselines(number_size, speedometer::GAUGE_UNIT_SIZE);
            draw_text(
                devices,
                &self.brush,
                unit,
                Vector2 {
                    X: center.x - unit.width / 2.0,
                    Y: center.y + unit_dy * scale - unit.baseline,
                },
                AMBER,
                UNIT_ALPHA,
            );
        }

        // The tachometer, and the row of information (detailed only).
        if let (Some(tach), Some(tach_parts)) = (&dashboard.tach, &parts.tach) {
            let rpm_fraction = speedometer::rpm_fraction(speedometer::engine_rpm(speed));
            draw_gauge(devices, &self.brush, tach, tach_parts, rpm_fraction, true)?;
            if let Some(caption) = &parts.caption {
                draw_text(
                    devices,
                    &self.brush,
                    caption,
                    Vector2 {
                        X: tach.center.x - caption.width / 2.0,
                        Y: tach.center.y + DIGITS_MIDDLE * caption.size - caption.baseline,
                    },
                    WHITE,
                    CAPTION_ALPHA,
                );
            }
        }
        if let Some(info) = dashboard.info
            && speedometer::text_visible(speedometer::INFO_TEXT_SIZE, scale)
        {
            self.texts
                .draw_info(devices, &self.brush, info, scene, scale, parts)?;
        }
        Ok(())
    }
}

impl Texts {
    /// The time, the distance driven and the gear in the row under the gauges.
    fn draw_info(
        &mut self,
        devices: &Devices,
        brush: &ID2D1SolidColorBrush,
        info: Rect,
        scene: &Scene,
        scale: f32,
        parts: &Parts,
    ) -> Result<()> {
        let size = speedometer::INFO_TEXT_SIZE * scale;
        let middle = (info.top + info.bottom) / 2.0;
        let clock = self.clock.get(
            devices.dwrite,
            &speedometer::clock_text(scene.minute_of_day()),
            size,
            INFO_FACE,
        )?;
        draw_text(
            devices,
            brush,
            clock,
            Vector2 {
                X: info.left,
                Y: middle + DIGITS_MIDDLE * size - clock.baseline,
            },
            WHITE,
            INFO_ALPHA,
        );
        let trip = self.trip.get(
            devices.dwrite,
            &speedometer::trip_text(scene.track_s()),
            size,
            INFO_FACE,
        )?;
        draw_text(
            devices,
            brush,
            trip,
            Vector2 {
                X: (info.left + info.right) / 2.0 - trip.width / 2.0,
                Y: middle + DIGITS_MIDDLE * size - trip.baseline,
            },
            WHITE,
            INFO_ALPHA,
        );
        if let Some(gear) = &parts.gear {
            draw_text(
                devices,
                brush,
                gear,
                Vector2 {
                    X: info.right - gear.width,
                    Y: middle + DIGITS_MIDDLE * size - gear.baseline,
                },
                AMBER,
                NUMBER_ALPHA,
            );
        }
        Ok(())
    }
}

fn fill_panel(devices: &Devices, brush: &ID2D1SolidColorBrush, panel: Rect, scale: f32) {
    let radius = speedometer::PANEL_CORNER * scale;
    set_brush(brush, BLACK, PANEL_ALPHA);
    unsafe {
        devices.context.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: panel.left,
                    top: panel.top,
                    right: panel.right,
                    bottom: panel.bottom,
                },
                radiusX: radius,
                radiusY: radius,
            },
            brush,
        );
    }
}

/// Draws a gauge: its scale, the arc up to `fraction`, and the needle.
fn draw_gauge(
    devices: &Devices,
    brush: &ID2D1SolidColorBrush,
    gauge: &Gauge,
    parts: &GaugeParts,
    fraction: f32,
    needle: bool,
) -> Result<()> {
    // The geometries are made around the center of the gauge.
    let around_origin = Gauge {
        center: Point { x: 0.0, y: 0.0 },
        ..*gauge
    };
    unsafe {
        devices
            .context
            .SetTransform(&Matrix3x2::translation(gauge.center.x, gauge.center.y));
    }
    for stroke in &parts.strokes {
        set_brush(brush, stroke.color, stroke.alpha);
        unsafe {
            devices.context.DrawGeometry(
                &stroke.geometry,
                brush,
                stroke.width,
                devices.stroke_style,
            );
        }
    }
    for label in &parts.labels {
        draw_text(
            devices,
            brush,
            &label.text,
            label.origin,
            WHITE,
            LABEL_ALPHA,
        );
    }
    if let Some(value) = arc_geometry(devices.factory, &around_origin, 0.0, fraction)? {
        set_brush(brush, AMBER, VALUE_ALPHA);
        unsafe {
            devices
                .context
                .DrawGeometry(&value, brush, gauge.stroke(), devices.stroke_style);
        }
    }
    if needle {
        let spec = &gauge.spec;
        let inner = around_origin.point_at(spec.needle_inner_inset, fraction);
        let outer = around_origin.point_at(spec.needle_outer_inset, fraction);
        set_brush(brush, WHITE, NEEDLE_ALPHA);
        unsafe {
            devices.context.DrawLine(
                vector(inner),
                vector(outer),
                brush,
                spec.needle_width * gauge.scale,
                devices.stroke_style,
            );
        }
    }
    unsafe {
        devices.context.SetTransform(&Matrix3x2::identity());
    }
    Ok(())
}

fn set_brush(brush: &ID2D1SolidColorBrush, color: D2D1_COLOR_F, alpha: f32) {
    unsafe {
        brush.SetColor(&color);
        brush.SetOpacity(alpha);
    }
}

fn draw_text(
    devices: &Devices,
    brush: &ID2D1SolidColorBrush,
    text: &Text,
    origin: Vector2,
    color: D2D1_COLOR_F,
    alpha: f32,
) {
    set_brush(brush, color, alpha);
    unsafe {
        devices
            .context
            .DrawTextLayout(origin, &text.layout, brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
    }
}

/// A line of text that is laid out.
struct Text {
    layout: IDWriteTextLayout,
    width: f32,
    /// The size of the font in pixels.
    size: f32,
    /// Distance from the top of the line to its baseline.
    baseline: f32,
}

impl Text {
    fn new(factory: &IDWriteFactory, text: &str, size: f32, face: Face) -> Result<Self> {
        let (layout, metrics, baseline) = layout(factory, text, size, face)?;
        Ok(Self {
            layout,
            width: metrics.width,
            size,
            baseline,
        })
    }
}

/// The last text that was laid out, for a text that changes now and then
/// (the speed, the time, the distance): it is laid out again only when it or
/// its size changes.
#[derive(Default)]
struct TextCache {
    entry: Option<(String, u32, Text)>,
}

impl TextCache {
    fn get(
        &mut self,
        factory: &IDWriteFactory,
        text: &str,
        size: f32,
        face: Face,
    ) -> Result<&Text> {
        let stale = self
            .entry
            .as_ref()
            .is_none_or(|(old, old_size, _)| old != text || *old_size != size.to_bits());
        if stale {
            let laid_out = Text::new(factory, text, size, face)?;
            self.entry = Some((text.to_owned(), size.to_bits(), laid_out));
        }
        match &self.entry {
            Some((_, _, text)) => Ok(text),
            None => Err(Error::empty()),
        }
    }
}

/// A geometry that is stroked, with how.
struct Stroke {
    geometry: ID2D1PathGeometry1,
    color: D2D1_COLOR_F,
    alpha: f32,
    width: f32,
}

/// A number of a scale, with where it is drawn relative to the center of its gauge.
struct Label {
    text: Text,
    origin: Vector2,
}

/// What a gauge has that does not change with its value.
struct GaugeParts {
    strokes: Vec<Stroke>,
    labels: Vec<Label>,
}

/// What the dashboard of one style has at one scale that does not change
/// with the speed.
struct Parts {
    speed: GaugeParts,
    /// The unit under the number in the speed gauge.
    unit: Option<Text>,
    tach: Option<GaugeParts>,
    caption: Option<Text>,
    gear: Option<Text>,
}

impl Parts {
    fn new(devices: &Devices, style: SpeedometerStyle, scale: f32) -> Result<Self> {
        let visible = |size: f32| speedometer::text_visible(size, scale);
        let text = |text: &str, size: f32, face: Face, shown: bool| -> Result<Option<Text>> {
            shown
                .then(|| Text::new(devices.dwrite, text, size * scale, face))
                .transpose()
        };
        let unit = text(
            "km/h",
            speedometer::GAUGE_UNIT_SIZE,
            UNIT_FACE,
            visible(speedometer::GAUGE_UNIT_SIZE),
        )?;
        if style == SpeedometerStyle::SimpleDashboard {
            return Ok(Self {
                speed: GaugeParts::new(devices, GaugeKind::Simple, scale)?,
                unit,
                tach: None,
                caption: None,
                gear: None,
            });
        }
        Ok(Self {
            speed: GaugeParts::new(devices, GaugeKind::Speed, scale)?,
            unit,
            tach: Some(GaugeParts::new(devices, GaugeKind::Tach, scale)?),
            caption: text(
                speedometer::TACH_CAPTION,
                speedometer::TACH_CAPTION_SIZE,
                UNIT_FACE,
                visible(speedometer::TACH_CAPTION_SIZE),
            )?,
            gear: text(
                speedometer::GEAR_TEXT,
                speedometer::INFO_TEXT_SIZE,
                INFO_FACE,
                visible(speedometer::INFO_TEXT_SIZE),
            )?,
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum GaugeKind {
    /// An arc only.
    Simple,
    /// The speed gauge of the detailed dashboard.
    Speed,
    /// The tachometer.
    Tach,
}

impl GaugeParts {
    fn new(devices: &Devices, kind: GaugeKind, scale: f32) -> Result<Self> {
        let spec: GaugeSpec = match kind {
            GaugeKind::Simple => SIMPLE_GAUGE,
            GaugeKind::Speed => SPEED_GAUGE,
            GaugeKind::Tach => TACH_GAUGE,
        };
        let gauge = Gauge {
            center: Point { x: 0.0, y: 0.0 },
            spec,
            scale,
        };
        let mut strokes = Vec::new();
        let mut labels = Vec::new();
        let mut push_arc = |from: f32, to: f32, color: D2D1_COLOR_F, alpha: f32| -> Result<()> {
            if let Some(geometry) = arc_geometry(devices.factory, &gauge, from, to)? {
                strokes.push(Stroke {
                    geometry,
                    color,
                    alpha,
                    width: gauge.stroke(),
                });
            }
            Ok(())
        };
        let red_from = speedometer::TACH_RED_RPM / speedometer::TACH_MAX_RPM;
        if kind == GaugeKind::Tach {
            push_arc(0.0, red_from, AMBER, TRACK_ALPHA)?;
            push_arc(red_from, 1.0, RED, RED_TRACK_ALPHA)?;
        } else {
            push_arc(0.0, 1.0, AMBER, TRACK_ALPHA)?;
        }

        if kind != GaugeKind::Simple {
            let ticks: Vec<Tick> = if kind == GaugeKind::Tach {
                speedometer::tach_ticks().collect()
            } else {
                speedometer::speed_ticks().collect()
            };
            // The ticks of one look make one geometry: the minor and the
            // major ones of the speed gauge, the amber and the red ones of
            // the tachometer.
            let is_red = |tick: &Tick| kind == GaugeKind::Tach && tick.fraction >= red_from;
            for (red, color, alpha, width, major) in [
                (false, AMBER, MINOR_TICK_ALPHA, MINOR_TICK_WIDTH, false),
                (false, AMBER, MAJOR_TICK_ALPHA, MAJOR_TICK_WIDTH, true),
                (true, RED, MAJOR_TICK_ALPHA, MAJOR_TICK_WIDTH, true),
            ] {
                let lines: Vec<(Point, Point)> = ticks
                    .iter()
                    .filter(|tick| tick.major == major && is_red(tick) == red)
                    .map(|tick| {
                        let length = if tick.major {
                            spec.major_tick
                        } else {
                            spec.minor_tick
                        };
                        (
                            gauge.point_at(spec.tick_inset, tick.fraction),
                            gauge.point_at(spec.tick_inset + length, tick.fraction),
                        )
                    })
                    .collect();
                if let Some(geometry) = lines_geometry(devices.factory, &lines)? {
                    strokes.push(Stroke {
                        geometry,
                        color,
                        alpha,
                        width: width * scale,
                    });
                }
            }

            if speedometer::text_visible(spec.label_size, scale) {
                for tick in ticks.iter().filter(|tick| tick.major) {
                    let size = spec.label_size * scale;
                    let text = Text::new(devices.dwrite, &tick.value.to_string(), size, UNIT_FACE)?;
                    let middle = gauge.point_at(spec.label_inset, tick.fraction);
                    let origin = Vector2 {
                        X: middle.x - text.width / 2.0,
                        Y: middle.y + DIGITS_MIDDLE * size - text.baseline,
                    };
                    labels.push(Label { text, origin });
                }
            }
        }
        Ok(Self { strokes, labels })
    }
}

fn vector(point: Point) -> Vector2 {
    Vector2 {
        X: point.x,
        Y: point.y,
    }
}

/// The arc of `gauge` from the place `from` to the place `to` of its scale
/// (0 to 1). `None` when the arc is too short to be seen.
fn arc_geometry(
    factory: &ID2D1Factory1,
    gauge: &Gauge,
    from: f32,
    to: f32,
) -> Result<Option<ID2D1PathGeometry1>> {
    let (from, to) = (from.clamp(0.0, 1.0), to.clamp(0.0, 1.0));
    // Less than a tenth of a degree is not a visible arc, and a zero length
    // would make an arc segment that has no direction.
    if (to - from) * speedometer::GAUGE_SWEEP_DEG < 0.1 {
        return Ok(None);
    }
    let radius = gauge.radius();
    let sweep = (to - from) * speedometer::GAUGE_SWEEP_DEG;
    unsafe {
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;
        sink.BeginFigure(vector(gauge.point_at(0.0, from)), D2D1_FIGURE_BEGIN_HOLLOW);
        sink.AddArc(&D2D1_ARC_SEGMENT {
            point: vector(gauge.point_at(0.0, to)),
            size: D2D_SIZE_F {
                width: radius,
                height: radius,
            },
            rotationAngle: 0.0,
            // Angles grow clockwise on the screen, like the scale.
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: if sweep > 180.0 {
                D2D1_ARC_SIZE_LARGE
            } else {
                D2D1_ARC_SIZE_SMALL
            },
        });
        sink.EndFigure(D2D1_FIGURE_END_OPEN);
        sink.Close()?;
        Ok(Some(geometry))
    }
}

/// One geometry of separate lines. `None` when there are no lines.
fn lines_geometry(
    factory: &ID2D1Factory1,
    lines: &[(Point, Point)],
) -> Result<Option<ID2D1PathGeometry1>> {
    if lines.is_empty() {
        return Ok(None);
    }
    unsafe {
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;
        for &(from, to) in lines {
            sink.BeginFigure(vector(from), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddLine(vector(to));
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
        }
        sink.Close()?;
        Ok(Some(geometry))
    }
}
