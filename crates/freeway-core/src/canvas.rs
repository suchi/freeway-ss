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

/// Surface the scene draws on. Lights are added to what is already drawn;
/// `silhouette` is the one command that covers it (or part of it). Coordinates and radii are
/// physical pixels. Filling the background is the renderer's job.
pub trait Canvas {
    /// A point light: a bright core of `core_radius` pixels with a halo of
    /// `GLOW_HALO_RATIO` times that radius around it.
    fn glow(&mut self, x: f32, y: f32, core_radius: f32, color: Rgb, alpha: f32);
    /// A soft elliptical pool of light lying on the road surface.
    fn light_pool(&mut self, x: f32, y: f32, radius_x: f32, radius_y: f32, color: Rgb, alpha: f32);
    /// A pool of light as `light_pool` draws it, but not inside any of the
    /// `shadows`, polygons through points of the screen where something stands
    /// in the way of the light. A renderer that cannot cut the shadows out draws
    /// the whole pool.
    #[allow(clippy::too_many_arguments)]
    fn shaded_light_pool(
        &mut self,
        x: f32,
        y: f32,
        radius_x: f32,
        radius_y: f32,
        color: Rgb,
        alpha: f32,
        shadows: &[Vec<(f32, f32)>],
    ) {
        let _ = shadows;
        self.light_pool(x, y, radius_x, radius_y, color, alpha);
    }
    /// A tiny solid point of `size` pixels, for stars and distant city lights.
    fn dot(&mut self, x: f32, y: f32, size: f32, color: Rgb, alpha: f32);
    /// A full-width horizontal band whose opacity rises from 0 at `top` to
    /// `alpha` at `peak` and falls back to 0 at `bottom`.
    fn sky_band(&mut self, top: f32, peak: f32, bottom: f32, color: Rgb, alpha: f32);
    /// A line of `width` pixels fading from transparent at the tail to
    /// `alpha` at the head, for shooting stars.
    fn streak(&mut self, tail: (f32, f32), head: (f32, f32), width: f32, color: Rgb, alpha: f32);
    /// A polygon through `points`, closed from the last point back to the
    /// first, filled with `color`: it covers what is already drawn behind it,
    /// as a mountain hides the stars. `alpha` is how much it covers, from 0
    /// (nothing) to 1 (all of it). Fewer than three points draw nothing.
    fn silhouette(&mut self, points: &[(f32, f32)], color: Rgb, alpha: f32);
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

/// The part of the picture that is drawn: lights wholly outside it are left
/// out. Coordinates are those of the picture, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Clip {
    /// The whole picture of `width` by `height` pixels.
    pub fn whole(width: f32, height: f32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: width,
            bottom: height,
        }
    }

    /// Whether an ellipse centered on (`x`, `y`) with the radii `radius_x`
    /// and `radius_y` overlaps the part. A point that is not finite does not.
    pub fn touches(&self, x: f32, y: f32, radius_x: f32, radius_y: f32) -> bool {
        x.is_finite()
            && y.is_finite()
            && x + radius_x >= self.left
            && x - radius_x <= self.right
            && y + radius_y >= self.top
            && y - radius_y <= self.bottom
    }
}

/// A canvas that draws what it is given moved by (`-dx`, `-dy`) onto another
/// canvas: the commands for a picture are in its own coordinates, and the
/// window that shows the part starting at (`dx`, `dy`) has its own.
pub struct ShiftCanvas<'a> {
    inner: &'a mut dyn Canvas,
    dx: f32,
    dy: f32,
}

impl<'a> ShiftCanvas<'a> {
    pub fn new(inner: &'a mut dyn Canvas, dx: f32, dy: f32) -> Self {
        Self { inner, dx, dy }
    }
}

impl Canvas for ShiftCanvas<'_> {
    fn glow(&mut self, x: f32, y: f32, core_radius: f32, color: Rgb, alpha: f32) {
        self.inner
            .glow(x - self.dx, y - self.dy, core_radius, color, alpha);
    }

    fn light_pool(&mut self, x: f32, y: f32, radius_x: f32, radius_y: f32, color: Rgb, alpha: f32) {
        self.inner
            .light_pool(x - self.dx, y - self.dy, radius_x, radius_y, color, alpha);
    }

    fn shaded_light_pool(
        &mut self,
        x: f32,
        y: f32,
        radius_x: f32,
        radius_y: f32,
        color: Rgb,
        alpha: f32,
        shadows: &[Vec<(f32, f32)>],
    ) {
        let moved: Vec<Vec<(f32, f32)>> = shadows
            .iter()
            .map(|shadow| self.moved(shadow).collect())
            .collect();
        self.inner.shaded_light_pool(
            x - self.dx,
            y - self.dy,
            radius_x,
            radius_y,
            color,
            alpha,
            &moved,
        );
    }

    fn dot(&mut self, x: f32, y: f32, size: f32, color: Rgb, alpha: f32) {
        self.inner.dot(x - self.dx, y - self.dy, size, color, alpha);
    }

    fn sky_band(&mut self, top: f32, peak: f32, bottom: f32, color: Rgb, alpha: f32) {
        self.inner.sky_band(
            top - self.dy,
            peak - self.dy,
            bottom - self.dy,
            color,
            alpha,
        );
    }

    fn streak(&mut self, tail: (f32, f32), head: (f32, f32), width: f32, color: Rgb, alpha: f32) {
        self.inner.streak(
            (tail.0 - self.dx, tail.1 - self.dy),
            (head.0 - self.dx, head.1 - self.dy),
            width,
            color,
            alpha,
        );
    }

    fn silhouette(&mut self, points: &[(f32, f32)], color: Rgb, alpha: f32) {
        let moved: Vec<(f32, f32)> = self.moved(points).collect();
        self.inner.silhouette(&moved, color, alpha);
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
        self.inner.beam(
            center_x - self.dx,
            top_y - self.dy,
            top_half_width,
            bottom_y - self.dy,
            bottom_half_width,
            color,
            top_alpha,
            bottom_alpha,
        );
    }
}

impl ShiftCanvas<'_> {
    fn moved<'p>(&self, points: &'p [(f32, f32)]) -> impl Iterator<Item = (f32, f32)> + 'p {
        let (dx, dy) = (self.dx, self.dy);
        points.iter().map(move |&(x, y)| (x - dx, y - dy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Log(Vec<String>);

    impl Canvas for Log {
        fn glow(&mut self, x: f32, y: f32, r: f32, _: Rgb, a: f32) {
            self.0.push(format!("glow {x} {y} {r} {a}"));
        }
        fn light_pool(&mut self, x: f32, y: f32, rx: f32, ry: f32, _: Rgb, a: f32) {
            self.0.push(format!("pool {x} {y} {rx} {ry} {a}"));
        }
        fn shaded_light_pool(
            &mut self,
            x: f32,
            y: f32,
            rx: f32,
            ry: f32,
            _: Rgb,
            a: f32,
            shadows: &[Vec<(f32, f32)>],
        ) {
            self.0
                .push(format!("shaded {x} {y} {rx} {ry} {a} {shadows:?}"));
        }
        fn dot(&mut self, x: f32, y: f32, size: f32, _: Rgb, a: f32) {
            self.0.push(format!("dot {x} {y} {size} {a}"));
        }
        fn sky_band(&mut self, top: f32, peak: f32, bottom: f32, _: Rgb, a: f32) {
            self.0.push(format!("band {top} {peak} {bottom} {a}"));
        }
        fn streak(&mut self, t: (f32, f32), h: (f32, f32), w: f32, _: Rgb, a: f32) {
            self.0.push(format!("streak {t:?} {h:?} {w} {a}"));
        }
        fn silhouette(&mut self, points: &[(f32, f32)], _: Rgb, a: f32) {
            self.0.push(format!("silhouette {points:?} {a}"));
        }
        fn beam(&mut self, x: f32, ty: f32, tw: f32, by: f32, bw: f32, _: Rgb, ta: f32, ba: f32) {
            self.0
                .push(format!("beam {x} {ty} {tw} {by} {bw} {ta} {ba}"));
        }
    }

    #[test]
    fn every_command_is_moved_by_the_origin_of_the_part() {
        let color = Rgb::new(1, 2, 3);
        let mut log = Log::default();
        {
            let mut shifted = ShiftCanvas::new(&mut log, 100.0, 40.0);
            shifted.glow(110.0, 50.0, 2.0, color, 0.5);
            shifted.light_pool(120.0, 60.0, 8.0, 3.0, color, 0.25);
            shifted.shaded_light_pool(
                130.0,
                70.0,
                9.0,
                4.0,
                color,
                0.125,
                &[vec![(140.0, 80.0), (150.0, 90.0)]],
            );
            shifted.dot(160.0, 100.0, 1.0, color, 1.0);
            shifted.sky_band(50.0, 60.0, 70.0, color, 0.5);
            shifted.streak((100.0, 100.0), (110.0, 120.0), 2.0, color, 1.0);
            shifted.silhouette(&[(100.0, 40.0), (200.0, 40.0), (150.0, 90.0)], color, 1.0);
            shifted.beam(150.0, 50.0, 5.0, 90.0, 20.0, color, 0.5, 0.25);
        }
        assert_eq!(
            log.0,
            [
                "glow 10 10 2 0.5",
                "pool 20 20 8 3 0.25",
                "shaded 30 30 9 4 0.125 [[(40.0, 40.0), (50.0, 50.0)]]",
                "dot 60 60 1 1",
                "band 10 20 30 0.5",
                "streak (0.0, 60.0) (10.0, 80.0) 2 1",
                "silhouette [(0.0, 0.0), (100.0, 0.0), (50.0, 50.0)] 1",
                "beam 50 10 5 50 20 0.5 0.25",
            ]
        );
    }

    #[test]
    fn a_part_touches_what_overlaps_it() {
        let clip = Clip {
            left: 100.0,
            top: 0.0,
            right: 200.0,
            bottom: 50.0,
        };
        assert!(clip.touches(150.0, 25.0, 1.0, 1.0));
        // Only the halo reaches in.
        assert!(clip.touches(95.0, 25.0, 5.0, 1.0));
        assert!(!clip.touches(94.0, 25.0, 5.0, 1.0));
        assert!(clip.touches(205.0, 25.0, 5.0, 1.0));
        assert!(!clip.touches(206.0, 25.0, 5.0, 1.0));
        assert!(!clip.touches(150.0, 60.0, 5.0, 5.0));
        assert!(!clip.touches(f32::NAN, 25.0, 5.0, 5.0));
        assert!(!clip.touches(150.0, f32::INFINITY, 5.0, 5.0));
        assert_eq!(
            Clip::whole(1280.0, 720.0),
            Clip {
                left: 0.0,
                top: 0.0,
                right: 1280.0,
                bottom: 720.0
            }
        );
    }
}
