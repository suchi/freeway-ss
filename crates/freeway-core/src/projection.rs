//! Perspective projection of road-relative points onto the screen.

/// How far ahead of the camera anything is drawn, in meters.
pub const VIEW_DISTANCE: f32 = 600.0;
/// Closest distance that can be projected, in meters.
pub const NEAR_DISTANCE: f32 = 1.0;
/// Lateral position of the camera: the middle of the left lane.
pub const CAMERA_LATERAL: f32 = -1.75;
/// Height of the camera above the road surface, in meters.
pub const CAMERA_HEIGHT: f32 = 1.2;

/// A point on the screen in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projected {
    pub x: f32,
    pub y: f32,
    /// Pixels per meter at the point's distance.
    pub scale: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Projector {
    focal: f32,
    center_x: f32,
    center_y: f32,
    left_hand_traffic: bool,
}

impl Projector {
    /// The vanishing point is the center of the screen.
    pub fn new(width: f32, height: f32, left_hand_traffic: bool) -> Self {
        Self {
            focal: (width * 0.5).max(height * 0.55),
            center_x: width * 0.5,
            center_y: height * 0.5,
            left_hand_traffic,
        }
    }

    pub fn focal(&self) -> f32 {
        self.focal
    }

    /// Projects a point `d` meters ahead. `lateral` and `height` are
    /// measured from the road's reference line and the road surface, already
    /// including the road's bend and slope. Returns `None` outside
    /// `NEAR_DISTANCE..VIEW_DISTANCE`.
    pub fn project(&self, d: f32, lateral: f32, height: f32) -> Option<Projected> {
        if !(NEAR_DISTANCE..VIEW_DISTANCE).contains(&d) {
            return None;
        }
        let scale = self.focal / d;
        // Right-hand traffic mirrors the whole layout around the camera.
        let side = if self.left_hand_traffic { 1.0 } else { -1.0 };
        Some(Projected {
            x: self.center_x + side * (lateral - CAMERA_LATERAL) * scale,
            y: self.center_y - (height - CAMERA_HEIGHT) * scale,
            scale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_straight_ahead_at_camera_height_hits_the_center() {
        let projector = Projector::new(1920.0, 1080.0, true);
        let p = projector
            .project(50.0, CAMERA_LATERAL, CAMERA_HEIGHT)
            .unwrap();
        assert_eq!((p.x, p.y), (960.0, 540.0));
        assert_eq!(p.scale, projector.focal() / 50.0);
    }

    #[test]
    fn focal_length_follows_the_larger_of_width_and_height() {
        assert_eq!(Projector::new(1000.0, 500.0, true).focal(), 500.0);
        assert_eq!(Projector::new(400.0, 1000.0, true).focal(), 550.0);
    }

    #[test]
    fn distance_outside_the_view_is_rejected() {
        let projector = Projector::new(800.0, 600.0, true);
        assert!(projector.project(0.99, 0.0, 0.0).is_none());
        assert!(projector.project(VIEW_DISTANCE, 0.0, 0.0).is_none());
        assert!(projector.project(NEAR_DISTANCE, 0.0, 0.0).is_some());
        assert!(projector.project(f32::NAN, 0.0, 0.0).is_none());
    }

    #[test]
    fn right_hand_traffic_mirrors_around_the_camera() {
        let left = Projector::new(800.0, 600.0, true);
        let right = Projector::new(800.0, 600.0, false);
        let a = left.project(30.0, 3.0, 0.0).unwrap();
        let b = right.project(30.0, 3.0, 0.0).unwrap();
        assert!((a.x - 400.0 + (b.x - 400.0)).abs() < 1e-3);
        assert_eq!(a.y, b.y);
    }

    #[test]
    fn higher_points_appear_higher_on_screen() {
        let projector = Projector::new(800.0, 600.0, true);
        let low = projector.project(30.0, 0.0, 0.0).unwrap();
        let high = projector.project(30.0, 0.0, 5.0).unwrap();
        assert!(high.y < low.y);
    }
}
