//! Direct2D rendering into one DXGI swap chain per window.

use std::collections::HashMap;
use std::mem::ManuallyDrop;

use freeway_core::canvas::GLOW_HALO_RATIO;
use freeway_core::{Canvas, Rgb, Scene, Viewport};

use crate::speedometer;

use windows::Win32::Foundation::{
    D2DERR_RECREATE_TARGET, DXGI_STATUS_OCCLUDED, HMODULE, HWND, RECT,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
    D2D1_FIGURE_END_CLOSED, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_BUFFER_PRECISION_8BPC_UNORM, D2D1_CAP_STYLE_ROUND, D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
    D2D1_COLOR_SPACE_SRGB, D2D1_DASH_STYLE_SOLID, D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_LINE_JOIN_ROUND, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    D2D1_PRIMITIVE_BLEND_ADD, D2D1_PRIMITIVE_BLEND_SOURCE_OVER,
    D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_STROKE_STYLE_PROPERTIES1, D2D1_UNIT_MODE_PIXELS,
    D2D1CreateFactory, ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Factory1,
    ID2D1GradientStopCollection1, ID2D1Image, ID2D1RadialGradientBrush, ID2D1SolidColorBrush,
    ID2D1StrokeStyle,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FEATURE, DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
    DWRITE_FONT_STRETCH, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_LIGHT, DWRITE_LINE_METRICS,
    DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory,
    IDWriteFactory, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_MWA_NO_ALT_ENTER, DXGI_PRESENT,
    DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2,
    IDXGISurface, IDXGISwapChain1,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::{Error, Interface, PCWSTR, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

/// Background color of the night sky (#02030a).
const BACKGROUND: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 2.0 / 255.0,
    g: 3.0 / 255.0,
    b: 10.0 / 255.0,
    a: 1.0,
};

/// Speedometer text, in pixels at a scale of 1 (see `speedometer::scale`).
/// The number is large and the unit small, both in amber like the dashboard of
/// the original.
const SPEEDOMETER_MARGIN: f32 = 28.0;
const SPEEDOMETER_NUMBER_SIZE: f32 = 56.0;
const SPEEDOMETER_UNIT_SIZE: f32 = 20.0;
const SPEEDOMETER_GAP: f32 = 8.0;
const SPEEDOMETER_NUMBER_ALPHA: f32 = 0.9;
const SPEEDOMETER_UNIT_ALPHA: f32 = 0.6;
/// Amber (#ffb000).
const SPEEDOMETER_COLOR: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 1.0,
    g: 176.0 / 255.0,
    b: 0.0,
    a: 1.0,
};

/// Bahnschrift, a DIN-style face that ships with Windows 10 version 1709 and
/// later. Where it is missing DirectWrite substitutes another face.
const SPEEDOMETER_FONT: PCWSTR = w!("Bahnschrift");

/// A light number looks clean at a large size; the small unit is a regular
/// weight so that it stays legible at its lower opacity.
const NUMBER_FACE: Face = Face {
    weight: DWRITE_FONT_WEIGHT_SEMI_LIGHT,
    stretch: DWRITE_FONT_STRETCH_NORMAL,
    tabular_figures: true,
};
const UNIT_FACE: Face = Face {
    weight: DWRITE_FONT_WEIGHT_NORMAL,
    stretch: DWRITE_FONT_STRETCH_NORMAL,
    tabular_figures: false,
};

/// Returns whether `error` means the GPU device is gone and every device
/// resource has to be created again.
pub fn is_device_lost(error: &Error) -> bool {
    [
        D2DERR_RECREATE_TARGET,
        DXGI_ERROR_DEVICE_REMOVED,
        DXGI_ERROR_DEVICE_RESET,
    ]
    .contains(&error.code())
}

/// Device-dependent resources shared by every window.
pub struct Renderer {
    d3d_device: ID3D11Device,
    dxgi_factory: IDXGIFactory2,
    /// Makes the path geometry of a beam.
    d2d_factory: ID2D1Factory1,
    context: ID2D1DeviceContext,
    glow_brushes: HashMap<Rgb, ID2D1RadialGradientBrush>,
    pool_brushes: HashMap<Rgb, ID2D1RadialGradientBrush>,
    dot_brushes: HashMap<Rgb, ID2D1SolidColorBrush>,
    /// Round caps, so that a shooting star has no square end.
    streak_style: ID2D1StrokeStyle,
    dwrite_factory: IDWriteFactory,
    speedometer_brush: ID2D1SolidColorBrush,
    /// Laid out text per (speed, scale); windows of another size or on another
    /// monitor may have another scale.
    speedometer_text: HashMap<(u32, u32), SpeedometerText>,
}

/// The text of the speedometer, laid out for one speed and scale.
struct SpeedometerText {
    number: IDWriteTextLayout,
    unit: IDWriteTextLayout,
    number_width: f32,
    number_height: f32,
    /// Distance from the top of each line to its baseline.
    number_baseline: f32,
    unit_baseline: f32,
}

/// The swap chain of one window and the Direct2D bitmap that wraps its back buffer.
pub struct Target {
    hwnd: HWND,
    swap_chain: IDXGISwapChain1,
    bitmap: Option<ID2D1Bitmap1>,
    size: (u32, u32),
}

impl Renderer {
    pub fn new() -> Result<Self> {
        let d3d_device = create_d3d_device()?;
        let dxgi_device: IDXGIDevice = d3d_device.cast()?;
        // Why: a swap chain must be created by the factory that owns the
        // device's adapter, so take the factory from the device instead of
        // creating a new one.
        let dxgi_factory: IDXGIFactory2 = unsafe { dxgi_device.GetAdapter()?.GetParent()? };

        let d2d_factory: ID2D1Factory1 =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let d2d_device = unsafe { d2d_factory.CreateDevice(&dxgi_device)? };
        let context = unsafe { d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)? };
        // Work in physical pixels; DPI scaling is applied by the scene itself.
        unsafe { context.SetUnitMode(D2D1_UNIT_MODE_PIXELS) };
        let streak_style = unsafe {
            d2d_factory.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES1 {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 1.0,
                    dashStyle: D2D1_DASH_STYLE_SOLID,
                    dashOffset: 0.0,
                    ..Default::default()
                },
                None,
            )?
        };

        let dwrite_factory: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
        let speedometer_brush = unsafe { context.CreateSolidColorBrush(&SPEEDOMETER_COLOR, None)? };

        Ok(Self {
            d3d_device,
            dxgi_factory,
            d2d_factory,
            context,
            glow_brushes: HashMap::new(),
            pool_brushes: HashMap::new(),
            dot_brushes: HashMap::new(),
            streak_style: streak_style.into(),
            dwrite_factory,
            speedometer_brush,
            speedometer_text: HashMap::new(),
        })
    }

    pub fn create_target(&self, hwnd: HWND) -> Result<Target> {
        let size = client_size(hwnd)?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: size.0,
            Height: size.1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            ..Default::default()
        };
        let swap_chain = unsafe {
            self.dxgi_factory
                .CreateSwapChainForHwnd(&self.d3d_device, hwnd, &desc, None, None)?
        };
        unsafe {
            self.dxgi_factory
                .MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER)?
        };
        let bitmap = self.create_target_bitmap(&swap_chain)?;
        Ok(Target {
            hwnd,
            swap_chain,
            bitmap: Some(bitmap),
            size,
        })
    }

    /// Draws `scene` into the window of `target` and presents it. With
    /// `speedometer_kmh` the speed is drawn in the lower left corner.
    ///
    /// With `wait_for_vsync` the call blocks until the frame can be shown,
    /// which paces the render loop. Returns `false` when the window is not
    /// visible, so that the caller can stop spinning.
    pub fn draw_scene(
        &mut self,
        target: &mut Target,
        scene: &Scene,
        speedometer_kmh: Option<u32>,
        wait_for_vsync: bool,
    ) -> Result<bool> {
        self.resize_if_needed(target)?;
        let viewport = Viewport {
            width: target.size.0 as f32,
            height: target.size.1 as f32,
            pixel_scale: pixel_scale(target.hwnd),
        };

        let Some(bitmap) = target.bitmap.as_ref() else {
            return Err(Error::empty());
        };
        let context = &self.context;
        unsafe {
            context.SetTarget(bitmap);
            context.BeginDraw();
            context.Clear(Some(&BACKGROUND));
            context.SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_ADD);
        }
        let mut canvas = D2dCanvas {
            context,
            factory: &self.d2d_factory,
            glow_brushes: &mut self.glow_brushes,
            pool_brushes: &mut self.pool_brushes,
            dot_brushes: &mut self.dot_brushes,
            streak_style: &self.streak_style,
            width: viewport.width,
            error: None,
        };
        scene.draw(viewport, &mut canvas);
        let mut brush_error = canvas.error;
        unsafe {
            context.SetTransform(&Matrix3x2::identity());
            context.SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_SOURCE_OVER);
        }
        if let Some(speed) = speedometer_kmh
            && let Err(error) = self.draw_speedometer(speed, viewport)
        {
            brush_error.get_or_insert(error);
        }
        let context = &self.context;
        unsafe {
            let ended = context.EndDraw(None, None);
            context.SetTarget(None::<&ID2D1Image>);
            ended?;
            if let Some(error) = brush_error {
                return Err(error);
            }
            let status = target
                .swap_chain
                .Present(u32::from(wait_for_vsync), DXGI_PRESENT(0));
            status.ok()?;
            Ok(status != DXGI_STATUS_OCCLUDED)
        }
    }

    /// Draws the speed over the scene with normal blending. Errors are
    /// returned for the caller to report after `EndDraw`.
    fn draw_speedometer(&mut self, speed_kmh: u32, viewport: Viewport) -> Result<()> {
        let scale = speedometer::scale(viewport.height, viewport.pixel_scale);
        if scale <= 0.0 {
            return Ok(());
        }
        let key = (speed_kmh, scale.to_bits());
        if !self.speedometer_text.contains_key(&key) {
            let text = SpeedometerText::new(&self.dwrite_factory, speed_kmh, scale)?;
            // The speed does not change while running, so this stays small.
            self.speedometer_text.insert(key, text);
        }
        let text = &self.speedometer_text[&key];
        let left = SPEEDOMETER_MARGIN * scale;
        let bottom = viewport.height - SPEEDOMETER_MARGIN * scale;
        let number_top = bottom - text.number_height;
        // Why: the two sizes share a baseline, not the bottom of the lines.
        let unit_top = number_top + text.number_baseline - text.unit_baseline;
        unsafe {
            self.context.SetTransform(&Matrix3x2::identity());
            self.speedometer_brush.SetOpacity(SPEEDOMETER_NUMBER_ALPHA);
            self.context.DrawTextLayout(
                Vector2 {
                    X: left,
                    Y: number_top,
                },
                &text.number,
                &self.speedometer_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            self.speedometer_brush.SetOpacity(SPEEDOMETER_UNIT_ALPHA);
            self.context.DrawTextLayout(
                Vector2 {
                    X: left + text.number_width + SPEEDOMETER_GAP * scale,
                    Y: unit_top,
                },
                &text.unit,
                &self.speedometer_brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
        Ok(())
    }

    fn resize_if_needed(&self, target: &mut Target) -> Result<()> {
        let size = client_size(target.hwnd)?;
        if size == target.size {
            return Ok(());
        }
        // The back buffers cannot be resized while a bitmap still refers to them.
        target.bitmap = None;
        unsafe {
            target.swap_chain.ResizeBuffers(
                0,
                size.0,
                size.1,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
        }
        target.bitmap = Some(self.create_target_bitmap(&target.swap_chain)?);
        target.size = size;
        Ok(())
    }

    fn create_target_bitmap(&self, swap_chain: &IDXGISwapChain1) -> Result<ID2D1Bitmap1> {
        let properties = D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
            colorContext: ManuallyDrop::new(None),
        };
        unsafe {
            let surface: IDXGISurface = swap_chain.GetBuffer(0)?;
            self.context
                .CreateBitmapFromDxgiSurface(&surface, Some(&properties))
        }
    }
}

impl SpeedometerText {
    fn new(factory: &IDWriteFactory, speed_kmh: u32, scale: f32) -> Result<Self> {
        let (number, number_metrics, number_baseline) = layout(
            factory,
            &speed_kmh.to_string(),
            SPEEDOMETER_NUMBER_SIZE * scale,
            NUMBER_FACE,
        )?;
        let (unit, _, unit_baseline) =
            layout(factory, "km/h", SPEEDOMETER_UNIT_SIZE * scale, UNIT_FACE)?;
        Ok(Self {
            number,
            unit,
            number_width: number_metrics.width,
            number_height: number_metrics.height,
            number_baseline,
            unit_baseline,
        })
    }
}

/// The look of one line of the speedometer.
#[derive(Clone, Copy)]
struct Face {
    weight: DWRITE_FONT_WEIGHT,
    stretch: DWRITE_FONT_STRETCH,
    /// Digits of equal width, so that the number does not shift when its
    /// digits change (the speed can change in the settings).
    tabular_figures: bool,
}

/// A single line of text in the speedometer font, with its size and baseline.
fn layout(
    factory: &IDWriteFactory,
    text: &str,
    size: f32,
    face: Face,
) -> Result<(IDWriteTextLayout, DWRITE_TEXT_METRICS, f32)> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        let format = factory.CreateTextFormat(
            SPEEDOMETER_FONT,
            None,
            face.weight,
            DWRITE_FONT_STYLE_NORMAL,
            face.stretch,
            size,
            w!("en-us"),
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        // The box is wide and tall enough that nothing is cut off.
        let layout = factory.CreateTextLayout(&wide, &format, size * 16.0, size * 2.0)?;
        if face.tabular_figures {
            let typography = factory.CreateTypography()?;
            typography.AddFontFeature(DWRITE_FONT_FEATURE {
                nameTag: DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
                parameter: 1,
            })?;
            layout.SetTypography(
                &typography,
                DWRITE_TEXT_RANGE {
                    startPosition: 0,
                    length: wide.len() as u32,
                },
            )?;
        }
        let mut metrics = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut metrics)?;
        let mut line = [DWRITE_LINE_METRICS::default()];
        let mut count = 0;
        layout.GetLineMetrics(Some(&mut line), &mut count)?;
        Ok((layout, metrics, line[0].baseline))
    }
}

/// Creates a hardware device, falling back to the WARP software rasterizer
/// on machines without a usable GPU driver (virtual machines, remote sessions).
fn create_d3d_device() -> Result<ID3D11Device> {
    let try_create = |driver_type: D3D_DRIVER_TYPE| -> Result<ID3D11Device> {
        let mut device = None;
        unsafe {
            D3D11CreateDevice(
                None,
                driver_type,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )?;
        }
        device.ok_or_else(Error::empty)
    };
    try_create(D3D_DRIVER_TYPE_HARDWARE).or_else(|_| try_create(D3D_DRIVER_TYPE_WARP))
}

/// Draws the scene's commands. Lights fill a unit circle, scaled into place
/// with the context transform, with a radial gradient brush per color; dots
/// use a solid brush per color; the sky band and the shooting stars use a
/// linear gradient brush made for each draw; a beam fills a trapezoid path
/// with such a brush.
struct D2dCanvas<'a> {
    context: &'a ID2D1DeviceContext,
    factory: &'a ID2D1Factory1,
    glow_brushes: &'a mut HashMap<Rgb, ID2D1RadialGradientBrush>,
    pool_brushes: &'a mut HashMap<Rgb, ID2D1RadialGradientBrush>,
    dot_brushes: &'a mut HashMap<Rgb, ID2D1SolidColorBrush>,
    streak_style: &'a ID2D1StrokeStyle,
    /// Width of the drawing area, which the sky band spans.
    width: f32,
    /// The first brush creation failure. `Canvas` cannot report errors, so
    /// the renderer checks this after the frame.
    error: Option<Error>,
}

impl D2dCanvas<'_> {
    fn fill_unit_circle(
        &self,
        brush: &ID2D1RadialGradientBrush,
        x: f32,
        y: f32,
        rx: f32,
        ry: f32,
        alpha: f32,
    ) {
        unsafe {
            brush.SetOpacity(alpha);
            self.context.SetTransform(&Matrix3x2 {
                M11: rx,
                M12: 0.0,
                M21: 0.0,
                M22: ry,
                M31: x,
                M32: y,
            });
            self.context.FillEllipse(
                &D2D1_ELLIPSE {
                    point: Vector2 { X: 0.0, Y: 0.0 },
                    radiusX: 1.0,
                    radiusY: 1.0,
                },
                brush,
            );
        }
    }
}

/// Looks up the brush for `color`, creating it on first use.
fn cached_brush(
    context: &ID2D1DeviceContext,
    cache: &mut HashMap<Rgb, ID2D1RadialGradientBrush>,
    color: Rgb,
    stops: &[(f32, f32, f32)],
    error: &mut Option<Error>,
) -> Option<ID2D1RadialGradientBrush> {
    if let Some(brush) = cache.get(&color) {
        return Some(brush.clone());
    }
    match create_brush(context, color, stops) {
        Ok(brush) => {
            cache.insert(color, brush.clone());
            Some(brush)
        }
        Err(e) => {
            error.get_or_insert(e);
            None
        }
    }
}

/// Gradient stops of a glow as (position, whiteness, alpha). The unit circle
/// is the halo; the core ends at `1 / GLOW_HALO_RATIO`. The center is whiter
/// than the color and the light drops off steeply outside the core.
const GLOW_STOPS: [(f32, f32, f32); 4] = [
    (0.0, 0.6, 1.0),
    (1.0 / GLOW_HALO_RATIO, 0.0, 0.7),
    (0.5, 0.0, 0.14),
    (1.0, 0.0, 0.0),
];
/// Gradient stops of a light pool: soft all the way to the edge.
const POOL_STOPS: [(f32, f32, f32); 3] = [(0.0, 0.0, 1.0), (0.5, 0.0, 0.35), (1.0, 0.0, 0.0)];

impl Canvas for D2dCanvas<'_> {
    fn glow(&mut self, x: f32, y: f32, core_radius: f32, color: Rgb, alpha: f32) {
        if let Some(brush) = cached_brush(
            self.context,
            self.glow_brushes,
            color,
            &GLOW_STOPS,
            &mut self.error,
        ) {
            let radius = core_radius * GLOW_HALO_RATIO;
            self.fill_unit_circle(&brush, x, y, radius, radius, alpha);
        }
    }

    fn light_pool(&mut self, x: f32, y: f32, radius_x: f32, radius_y: f32, color: Rgb, alpha: f32) {
        if let Some(brush) = cached_brush(
            self.context,
            self.pool_brushes,
            color,
            &POOL_STOPS,
            &mut self.error,
        ) {
            self.fill_unit_circle(&brush, x, y, radius_x, radius_y, alpha);
        }
    }

    fn dot(&mut self, x: f32, y: f32, size: f32, color: Rgb, alpha: f32) {
        let brush = match self.dot_brushes.get(&color) {
            Some(brush) => brush.clone(),
            None => match unsafe {
                self.context
                    .CreateSolidColorBrush(&color_f(color, 1.0), None)
            } {
                Ok(brush) => {
                    self.dot_brushes.insert(color, brush.clone());
                    brush
                }
                Err(e) => {
                    self.error.get_or_insert(e);
                    return;
                }
            },
        };
        let half = size * 0.5;
        unsafe {
            brush.SetOpacity(alpha);
            self.context.SetTransform(&Matrix3x2::identity());
            self.context.FillRectangle(
                &D2D_RECT_F {
                    left: x - half,
                    top: y - half,
                    right: x + half,
                    bottom: y + half,
                },
                &brush,
            );
        }
    }

    fn sky_band(&mut self, top: f32, peak: f32, bottom: f32, color: Rgb, alpha: f32) {
        if !(top < peak && peak < bottom) {
            return;
        }
        let stops = [
            (0.0, color_f(color, 0.0)),
            ((peak - top) / (bottom - top), color_f(color, alpha)),
            (1.0, color_f(color, 0.0)),
        ];
        let properties = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: Vector2 { X: 0.0, Y: top },
            endPoint: Vector2 { X: 0.0, Y: bottom },
        };
        let brush = stop_collection(self.context, &stops).and_then(|collection| unsafe {
            self.context
                .CreateLinearGradientBrush(&properties, None, &collection)
        });
        match brush {
            Ok(brush) => unsafe {
                self.context.SetTransform(&Matrix3x2::identity());
                self.context.FillRectangle(
                    &D2D_RECT_F {
                        left: 0.0,
                        top,
                        right: self.width,
                        bottom,
                    },
                    &brush,
                );
            },
            Err(e) => {
                self.error.get_or_insert(e);
            }
        }
    }

    fn streak(&mut self, tail: (f32, f32), head: (f32, f32), width: f32, color: Rgb, alpha: f32) {
        // A gradient needs two distinct points.
        if (head.0 - tail.0).hypot(head.1 - tail.1) < 1.0 {
            return;
        }
        let stops = [(0.0, color_f(color, 0.0)), (1.0, color_f(color, alpha))];
        let properties = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: Vector2 {
                X: tail.0,
                Y: tail.1,
            },
            endPoint: Vector2 {
                X: head.0,
                Y: head.1,
            },
        };
        let brush = stop_collection(self.context, &stops).and_then(|collection| unsafe {
            self.context
                .CreateLinearGradientBrush(&properties, None, &collection)
        });
        match brush {
            Ok(brush) => unsafe {
                self.context.SetTransform(&Matrix3x2::identity());
                self.context.DrawLine(
                    Vector2 {
                        X: tail.0,
                        Y: tail.1,
                    },
                    Vector2 {
                        X: head.0,
                        Y: head.1,
                    },
                    &brush,
                    width,
                    self.streak_style,
                );
            },
            Err(e) => {
                self.error.get_or_insert(e);
            }
        }
    }

    fn beam(
        &mut self,
        center_x: f32,
        top_y: f32,
        top_half_width: f32,
        bottom_y: f32,
        bottom_half_width: f32,
        color: Rgb,
        top_alpha: f32,
        bottom_alpha: f32,
    ) {
        // A gradient needs two distinct points.
        if bottom_y - top_y < 1.0 {
            return;
        }
        let stops = [
            (0.0, color_f(color, top_alpha)),
            (1.0, color_f(color, bottom_alpha)),
        ];
        let properties = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: Vector2 {
                X: center_x,
                Y: top_y,
            },
            endPoint: Vector2 {
                X: center_x,
                Y: bottom_y,
            },
        };
        let corners = [
            Vector2 {
                X: center_x + top_half_width,
                Y: top_y,
            },
            Vector2 {
                X: center_x + bottom_half_width,
                Y: bottom_y,
            },
            Vector2 {
                X: center_x - bottom_half_width,
                Y: bottom_y,
            },
            Vector2 {
                X: center_x - top_half_width,
                Y: top_y,
            },
        ];
        let result = stop_collection(self.context, &stops).and_then(|collection| unsafe {
            let brush = self
                .context
                .CreateLinearGradientBrush(&properties, None, &collection)?;
            let geometry = self.factory.CreatePathGeometry()?;
            let sink = geometry.Open()?;
            sink.BeginFigure(corners[0], D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLines(&corners[1..]);
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            self.context.SetTransform(&Matrix3x2::identity());
            self.context.FillGeometry(&geometry, &brush, None);
            Ok(())
        });
        if let Err(e) = result {
            self.error.get_or_insert(e);
        }
    }
}

fn color_f(color: Rgb, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: alpha,
    }
}

/// Gradient stops from (position, color) pairs, interpolated without premultiplying.
fn stop_collection(
    context: &ID2D1DeviceContext,
    stops: &[(f32, D2D1_COLOR_F)],
) -> Result<ID2D1GradientStopCollection1> {
    let stops: Vec<D2D1_GRADIENT_STOP> = stops
        .iter()
        .map(|&(position, color)| D2D1_GRADIENT_STOP { position, color })
        .collect();
    unsafe {
        context.CreateGradientStopCollection(
            &stops,
            D2D1_COLOR_SPACE_SRGB,
            D2D1_COLOR_SPACE_SRGB,
            D2D1_BUFFER_PRECISION_8BPC_UNORM,
            D2D1_EXTEND_MODE_CLAMP,
            D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
        )
    }
}

/// A unit-radius radial gradient of `color` centered on the origin.
fn create_brush(
    context: &ID2D1DeviceContext,
    color: Rgb,
    stops: &[(f32, f32, f32)],
) -> Result<ID2D1RadialGradientBrush> {
    let channel = |value: u8, whiteness: f32| {
        let value = f32::from(value) / 255.0;
        value + (1.0 - value) * whiteness
    };
    let stops: Vec<(f32, D2D1_COLOR_F)> = stops
        .iter()
        .map(|&(position, whiteness, a)| {
            let color = D2D1_COLOR_F {
                r: channel(color.r, whiteness),
                g: channel(color.g, whiteness),
                b: channel(color.b, whiteness),
                a,
            };
            (position, color)
        })
        .collect();
    let collection = stop_collection(context, &stops)?;
    unsafe {
        context.CreateRadialGradientBrush(
            &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: Vector2 { X: 0.0, Y: 0.0 },
                gradientOriginOffset: Vector2 { X: 0.0, Y: 0.0 },
                radiusX: 1.0,
                radiusY: 1.0,
            },
            None,
            &collection,
        )
    }
}

/// DPI of the window relative to 96, which scales sizes defined on screen.
fn pixel_scale(hwnd: HWND) -> f32 {
    // GetDpiForWindow returns 0 for an invalid window.
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => 1.0,
        dpi => dpi as f32 / 96.0,
    }
}

/// Client area size in physical pixels, at least 1x1 because a swap chain
/// cannot have an empty buffer.
fn client_size(hwnd: HWND) -> Result<(u32, u32)> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect)? };
    let width = u32::try_from(rect.right - rect.left).unwrap_or(0).max(1);
    let height = u32::try_from(rect.bottom - rect.top).unwrap_or(0).max(1);
    Ok((width, height))
}
