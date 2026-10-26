//! Drawing commands that the scene emits and a renderer carries out.

/// An sRGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// How many times larger the halo of a glow is than its core. The scene uses
/// it to decide whether a glow touches the screen.
pub const GLOW_HALO_RATIO: f32 = 4.0;

/// Surface the scene draws on. All lights are added to what is already
/// drawn; coordinates and radii are physical pixels. Filling the background
/// is the renderer's job.
pub trait Canvas {
    /// A point light: a bright core of `core_radius` pixels with a halo of
    /// `GLOW_HALO_RATIO` times that radius around it.
    fn glow(&mut self, x: f32, y: f32, core_radius: f32, color: Rgb, alpha: f32);
    /// A soft elliptical pool of light lying on the road surface.
    fn light_pool(&mut self, x: f32, y: f32, radius_x: f32, radius_y: f32, color: Rgb, alpha: f32);
    /// A tiny solid point of `size` pixels, for stars and distant city lights.
    fn dot(&mut self, x: f32, y: f32, size: f32, color: Rgb, alpha: f32);
    /// A full-width horizontal band whose opacity rises from 0 at `top` to
    /// `alpha` at `peak` and falls back to 0 at `bottom`.
    fn sky_band(&mut self, top: f32, peak: f32, bottom: f32, color: Rgb, alpha: f32);
    /// A line of `width` pixels fading from transparent at the tail to
    /// `alpha` at the head, for shooting stars.
    fn streak(&mut self, tail: (f32, f32), head: (f32, f32), width: f32, color: Rgb, alpha: f32);
    /// A trapezoid of light, as a beam shining down from a ship. It spans
    /// from `top_y` to `bottom_y`, centered on `center_x`, with half the
    /// width at each end given by `top_half_width` and `bottom_half_width`.
    /// Its opacity changes evenly from `top_alpha` to `bottom_alpha`.
    #[allow(clippy::too_many_arguments)]
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
    );
}
