//! Direct2D rendering into one DXGI swap chain per window.

use std::mem::ManuallyDrop;

use windows::Win32::Foundation::{
    D2DERR_RECREATE_TARGET, DXGI_STATUS_OCCLUDED, HMODULE, HWND, RECT,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_BUFFER_PRECISION_8BPC_UNORM, D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
    D2D1_COLOR_SPACE_SRGB, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_PRIMITIVE_BLEND_ADD, D2D1_PRIMITIVE_BLEND_SOURCE_OVER,
    D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_UNIT_MODE_PIXELS, D2D1CreateFactory, ID2D1Bitmap1,
    ID2D1DeviceContext, ID2D1Factory1, ID2D1Image, ID2D1RadialGradientBrush,
};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device,
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
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::{Error, Interface, Result};
use windows_numerics::{Matrix3x2, Vector2};

/// Background color of the night sky (#02030a).
const BACKGROUND: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 2.0 / 255.0,
    g: 3.0 / 255.0,
    b: 10.0 / 255.0,
    a: 1.0,
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
    context: ID2D1DeviceContext,
    glow: ID2D1RadialGradientBrush,
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

        let glow = create_glow_brush(&context)?;
        Ok(Self {
            d3d_device,
            dxgi_factory,
            context,
            glow,
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

    /// Draws the Phase 1 test pattern: a glow that crosses the window every
    /// five seconds, blended additively like the lights of the final scene.
    ///
    /// With `wait_for_vsync` the call blocks until the frame can be shown,
    /// which paces the render loop. Returns `false` when the window is not
    /// visible, so that the caller can stop spinning.
    pub fn draw_test_frame(
        &self,
        target: &mut Target,
        seconds: f32,
        wait_for_vsync: bool,
    ) -> Result<bool> {
        self.resize_if_needed(target)?;
        let (width, height) = (target.size.0 as f32, target.size.1 as f32);
        let radius = height.max(1.0) * 0.03;
        let x = (seconds / 5.0).fract() * width;
        let y = height * 0.5;

        let Some(bitmap) = target.bitmap.as_ref() else {
            return Err(Error::empty());
        };
        let context = &self.context;
        unsafe {
            context.SetTarget(bitmap);
            context.BeginDraw();
            context.Clear(Some(&BACKGROUND));
            context.SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_ADD);
            context.SetTransform(&Matrix3x2 {
                M11: radius,
                M12: 0.0,
                M21: 0.0,
                M22: radius,
                M31: x,
                M32: y,
            });
            context.FillEllipse(
                &D2D1_ELLIPSE {
                    point: Vector2 { X: 0.0, Y: 0.0 },
                    radiusX: 1.0,
                    radiusY: 1.0,
                },
                &self.glow,
            );
            context.SetTransform(&Matrix3x2::identity());
            context.SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_SOURCE_OVER);
            let ended = context.EndDraw(None, None);
            context.SetTarget(None::<&ID2D1Image>);
            ended?;
            let status = target
                .swap_chain
                .Present(u32::from(wait_for_vsync), DXGI_PRESENT(0));
            status.ok()?;
            Ok(status != DXGI_STATUS_OCCLUDED)
        }
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

/// A unit-radius glow centered on the origin: a white-hot core fading out
/// through a warm halo, scaled into place with the context transform.
fn create_glow_brush(context: &ID2D1DeviceContext) -> Result<ID2D1RadialGradientBrush> {
    let stop = |position: f32, r: f32, g: f32, b: f32, a: f32| D2D1_GRADIENT_STOP {
        position,
        color: D2D1_COLOR_F { r, g, b, a },
    };
    let stops = [
        stop(0.0, 1.0, 0.95, 0.85, 1.0),
        stop(0.15, 1.0, 0.7, 0.35, 0.8),
        stop(0.45, 1.0, 0.6, 0.25, 0.2),
        stop(1.0, 1.0, 0.6, 0.25, 0.0),
    ];
    unsafe {
        let collection = context.CreateGradientStopCollection(
            &stops,
            D2D1_COLOR_SPACE_SRGB,
            D2D1_COLOR_SPACE_SRGB,
            D2D1_BUFFER_PRECISION_8BPC_UNORM,
            D2D1_EXTEND_MODE_CLAMP,
            D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
        )?;
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

/// Client area size in physical pixels, at least 1x1 because a swap chain
/// cannot have an empty buffer.
fn client_size(hwnd: HWND) -> Result<(u32, u32)> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect)? };
    let width = u32::try_from(rect.right - rect.left).unwrap_or(0).max(1);
    let height = u32::try_from(rect.bottom - rect.top).unwrap_or(0).max(1);
    Ok((width, height))
}
