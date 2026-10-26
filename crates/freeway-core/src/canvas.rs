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
}
