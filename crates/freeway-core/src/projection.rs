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
    /// `1.0` for left-hand traffic, `-1.0` for right-hand traffic, and in
    /// between while the picture turns over from one to the other.
    side: f32,
    /// Lateral position of the camera.
    camera: f32,
    /// How far the camera is turned toward larger lateral positions, in
    /// radians; points then move the other way on the screen.
    yaw: f32,
}

impl Projector {
    /// The vanishing point is the center of the screen.
    pub fn new(width: f32, height: f32, left_hand_traffic: bool) -> Self {
        Self::with_side(width, height, if left_hand_traffic { 1.0 } else { -1.0 })
    }

    /// Like `new`, with the side given as a number from `-1.0` (right-hand
    /// traffic) to `1.0` (left-hand traffic); the values in between are the
    /// picture while it turns over.
    pub fn with_side(width: f32, height: f32, side: f32) -> Self {
        Self {
            focal: (width * 0.5).max(height * 0.55),
            center_x: width * 0.5,
            center_y: height * 0.5,
            side,
            camera: CAMERA_LATERAL,
            yaw: 0.0,
        }
    }

    /// The same projection seen from `lateral` instead of the middle of the
    /// left lane, with the camera turned by `yaw` radians toward larger
    /// lateral positions.
    pub fn with_camera(self, lateral: f32, yaw: f32) -> Self {
        Self {
            camera: lateral,
            yaw,
            ..self
        }
    }

    /// Lateral position of the camera.
    pub fn camera_lateral(&self) -> f32 {
        self.camera
    }

    /// `1.0` where the screen's x grows with the lateral position (left-hand
    /// traffic), `-1.0` where it shrinks (right-hand traffic).
    pub fn side(&self) -> f32 {
        self.side
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
        let across = lateral - self.camera;
        // The camera turned by `yaw` sees a point at `across` to the side and
        // `d` ahead at the angle between them.
        let across = if self.yaw == 0.0 {
            across
        } else {
            let (sin, cos) = self.yaw.sin_cos();
            d * (across * cos - d * sin) / (d * cos + across * sin)
        };
        // Right-hand traffic mirrors the whole layout around the camera.
        Some(Projected {
            x: self.center_x + self.side() * across * scale,
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
    fn the_side_in_between_squeezes_the_picture_toward_the_camera() {
        let half = Projector::with_side(800.0, 600.0, 0.5);
        let full = Projector::with_side(800.0, 600.0, 1.0);
        let a = half.project(30.0, 3.0, 0.0).unwrap();
        let b = full.project(30.0, 3.0, 0.0).unwrap();
        assert!(((a.x - 400.0) * 2.0 - (b.x - 400.0)).abs() < 1e-3);
        assert_eq!(a.y, b.y);
        assert_eq!(half.side(), 0.5);
    }

    #[test]
    fn the_camera_follows_its_lateral_position() {
        let moved = Projector::new(800.0, 600.0, true).with_camera(1.75, 0.0);
        let p = moved.project(50.0, 1.75, CAMERA_HEIGHT).unwrap();
        assert_eq!((p.x, p.y), (400.0, 300.0));
        assert_eq!(moved.camera_lateral(), 1.75);
    }

    #[test]
    fn a_turned_camera_moves_points_the_other_way() {
        let straight = Projector::new(800.0, 600.0, true);
        let turned = straight.with_camera(CAMERA_LATERAL, 0.02);
        let a = straight.project(30.0, 3.0, 0.0).unwrap();
        let b = turned.project(30.0, 3.0, 0.0).unwrap();
        // About `focal * yaw` pixels to the left, for small angles.
        let shift = a.x - b.x;
        assert!((shift - straight.focal() * 0.02).abs() < 2.0, "{shift}");
        assert_eq!(a.y, b.y);
        // The straight-ahead point of the turned camera is the center.
        let ahead = turned
            .with_camera(CAMERA_LATERAL, 0.02)
            .project(40.0, CAMERA_LATERAL + 40.0 * 0.02_f32.tan(), CAMERA_HEIGHT)
            .unwrap();
        assert!((ahead.x - 400.0).abs() < 0.5, "{}", ahead.x);
    }

    #[test]
    fn higher_points_appear_higher_on_screen() {
        let projector = Projector::new(800.0, 600.0, true);
        let low = projector.project(30.0, 0.0, 0.0).unwrap();
        let high = projector.project(30.0, 0.0, 5.0).unwrap();
        assert!(high.y < low.y);
    }
}
