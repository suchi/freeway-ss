//! Sizing of the speedometer, independent of the OS so that it is tested everywhere.

/// Height in logical pixels (at 96 DPI) from which the speedometer has its
/// full size. This is below the height of any real full-screen display, so
/// the speedometer only shrinks in small windows such as the preview in the
/// screensaver settings (about 152 x 112 pixels).
const FULL_SIZE_HEIGHT: f32 = 540.0;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
