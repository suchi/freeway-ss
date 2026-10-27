//! Where the faces of the bodies of cars stand in front of the lights.
//!
//! A face is a rectangle that lies in one of the planes of the road's
//! coordinates: an end at one distance, a side at one lateral position, a
//! roof at one height. Its corners are laid out like the lights of the road
//! (bent and lifted with it), so the distance of the face at a point of the
//! screen is found by projecting along the road.

use crate::projection::Projector;
use crate::road::RoadView;

/// The corners of a face as (distance, lateral position, height).
pub type Corners = [(f32, f32, f32); 4];

/// Whether the face turns toward the camera, which is when its corners, seen
/// on the screen from the outside, run the way the corners of a face in front
/// of the camera run (counterclockwise for left-hand traffic, which the
/// screen mirrors for right-hand traffic). A face with a corner that cannot be
/// projected does not turn toward it.
pub fn faces_camera(corners: &Corners, projector: &Projector, view: &RoadView) -> bool {
    let mut points = [(0.0, 0.0); 4];
    for (point, &(d, lateral, height)) in points.iter_mut().zip(corners) {
        let (bend, rise) = view.offset(d);
        let Some(p) = projector.project(d, lateral + bend, height + rise) else {
            return false;
        };
        *point = (p.x, p.y);
    }
    // Twice the signed area of the polygon, with y pointing down; measured
    // from a corner, since far away the faces are small and edge-on.
    let origin = points[0];
    let area: f32 = (0..4)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % 4]);
            (a.0 - origin.0) * (b.1 - origin.1) - (b.0 - origin.0) * (a.1 - origin.1)
        })
        .sum();
    area * projector.side() < 0.0
}

/// The distance of the face at the point `(x, y)` of the screen; `None`
/// where the face does not cover the point.
pub fn depth_at(
    corners: &Corners,
    projector: &Projector,
    view: &RoadView,
    (x, y): (f32, f32),
) -> Option<f32> {
    let project = |d: f32, lateral: f32, height: f32| {
        let (bend, rise) = view.offset(d);
        projector.project(d, lateral + bend, height + rise)
    };
    let range = |axis: fn(&(f32, f32, f32)) -> f32| {
        let values = corners.iter().map(axis);
        (
            values.clone().fold(f32::MAX, f32::min),
            values.fold(f32::MIN, f32::max),
        )
    };
    let constant = |axis: fn(&(f32, f32, f32)) -> f32| {
        let (low, high) = range(axis);
        low == high
    };
    let (near, far) = range(|c| c.0);
    let (left, right) = range(|c| c.1);
    let (bottom, top) = range(|c| c.2);
    if constant(|c| c.0) {
        let mut points = [(0.0, 0.0); 4];
        for (point, &(d, lateral, height)) in points.iter_mut().zip(corners) {
            let p = project(d, lateral, height)?;
            *point = (p.x, p.y);
        }
        return contains(&points, (x, y)).then_some(near);
    }
    if constant(|c| c.1) {
        // The side stands at one lateral position: the screen x tells the distance.
        let d = solve(near, far, |d| Some(project(d, left, 0.0)?.x - x))?;
        let (low, high) = (project(d, left, bottom)?.y, project(d, left, top)?.y);
        return (low.min(high)..=low.max(high)).contains(&y).then_some(d);
    }
    if constant(|c| c.2) {
        // The roof lies at one height: the screen y tells the distance.
        let d = solve(near, far, |d| Some(project(d, left, bottom)?.y - y))?;
        let (a, b) = (project(d, left, bottom)?.x, project(d, right, bottom)?.x);
        return (a.min(b)..=a.max(b)).contains(&x).then_some(d);
    }
    None
}

/// Whether the point is inside the quadrilateral, whichever way it runs.
fn contains(points: &[(f32, f32); 4], (x, y): (f32, f32)) -> bool {
    let sides = (0..4).map(|i| {
        let (a, b) = (points[i], points[(i + 1) % 4]);
        (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0)
    });
    let (mut before, mut after) = (false, false);
    for side in sides {
        before |= side < 0.0;
        after |= side > 0.0;
    }
    !(before && after)
}

/// The `d` in `low..=high` at which `f` changes its sign, found by halving;
/// `None` where it has the same sign at both ends.
fn solve(mut low: f32, mut high: f32, f: impl Fn(f32) -> Option<f32>) -> Option<f32> {
    let (at_low, at_high) = (f(low)?, f(high)?);
    if at_low * at_high > 0.0 {
        return None;
    }
    for _ in 0..20 {
        let middle = 0.5 * (low + high);
        if f(middle)? * at_low > 0.0 {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(0.5 * (low + high))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projection::CAMERA_HEIGHT;

    fn projector() -> Projector {
        Projector::new(1280.0, 720.0, true)
    }

    fn at(corners: &Corners, d: f32, lateral: f32, height: f32, shift: (f32, f32)) -> Option<f32> {
        let view = RoadView::flat();
        let p = projector().project(d, lateral, height)?;
        depth_at(corners, &projector(), &view, (p.x + shift.0, p.y + shift.1))
    }

    #[test]
    fn an_end_face_has_one_distance_over_its_area() {
        let end = [
            (20.0, 0.85, 0.0),
            (20.0, 2.65, 0.0),
            (20.0, 2.65, 1.45),
            (20.0, 0.85, 1.45),
        ];
        assert_eq!(at(&end, 20.0, 1.75, 0.7, (0.0, 0.0)), Some(20.0));
        assert_eq!(at(&end, 20.0, 1.75, 0.7, (200.0, 0.0)), None);
        assert_eq!(at(&end, 20.0, 1.75, 0.7, (0.0, -200.0)), None);
    }

    #[test]
    fn a_side_face_has_the_distance_of_the_point_it_shows() {
        // The side of a car in the passing lane, 20 to 24.5 meters ahead.
        let side = [
            (20.0, 0.85, 0.0),
            (24.5, 0.85, 0.0),
            (24.5, 0.85, 1.45),
            (20.0, 0.85, 1.45),
        ];
        for d in [20.0, 21.0, 22.7, 24.5] {
            let found = at(&side, d, 0.85, 0.6, (0.0, 0.0)).unwrap();
            assert!((found - d).abs() < 1e-2, "{d}: {found}");
        }
        // Beyond the ends and above the roof there is nothing of it.
        assert_eq!(at(&side, 26.0, 0.85, 0.6, (0.0, 0.0)), None);
        assert_eq!(at(&side, 22.0, 0.85, 2.5, (0.0, 0.0)), None);
        // A light behind the side, over the road beyond the car, is covered.
        let (found, light) = (at(&side, 22.0, 0.85, 0.6, (0.0, 0.0)).unwrap(), 40.0);
        assert!(found < light);
    }

    #[test]
    fn a_roof_has_the_distance_of_the_point_it_shows() {
        // A roof below the eye, as seen when the road runs downhill.
        let height = CAMERA_HEIGHT - 0.5;
        let roof = [
            (20.0, 0.85, height),
            (20.0, 2.65, height),
            (24.5, 2.65, height),
            (24.5, 0.85, height),
        ];
        for d in [20.5, 22.0, 24.0] {
            let found = at(&roof, d, 1.5, height, (0.0, 0.0)).unwrap();
            assert!((found - d).abs() < 1e-2, "{d}: {found}");
        }
        assert_eq!(at(&roof, 22.0, 5.0, height, (0.0, 0.0)), None);
    }

    #[test]
    fn a_light_behind_a_car_that_drives_by_is_covered_once() {
        // A delineator of the median, 60 meters ahead. The car in the passing
        // lane comes from afar, passes the light's direction and goes on.
        let view = RoadView::flat();
        let p = projector().project(60.0, 3.9, 0.9).unwrap();
        let mut covered = Vec::new();
        for step in 0..600 {
            let near = 2.0 + step as f32 * 0.1;
            let side = [
                (near, 0.85, 0.0),
                (near + 4.5, 0.85, 0.0),
                (near + 4.5, 0.85, 1.45),
                (near, 0.85, 1.45),
            ];
            let depth = depth_at(&side, &projector(), &view, (p.x, p.y));
            covered.push(depth.is_some_and(|d| d < 60.0));
        }
        let changes = covered.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(covered.iter().any(|&c| c), "never covered");
        assert!(changes <= 2, "{changes}");
    }

    use crate::projection::CAMERA_LATERAL;
    use crate::traffic::{FacePart, Traffic, TruckStyle};

    /// The faces of the bodies of the traffic that the camera sees, over a drive,
    /// as (face, whether it is seen, whether it is the right side of its box),
    /// with the road `view` and the traffic on the side of `left_hand_traffic`.
    fn faces_seen(
        view: &RoadView,
        left_hand_traffic: bool,
        mut each: impl FnMut(&crate::traffic::BodyFace, bool, bool),
    ) {
        let projector = Projector::new(1280.0, 720.0, left_hand_traffic);
        let mut traffic = Traffic::new(3, TruckStyle::Japanese);
        for _ in 0..2400 {
            traffic.update(0.05, 27.0);
            for body in traffic.boxes() {
                for face in body.faces() {
                    // A face cut off at the end of the view is a sliver of no area.
                    let (near, far) = face
                        .corners
                        .iter()
                        .fold((f32::MAX, f32::MIN), |(n, f), c| (n.min(c.0), f.max(c.0)));
                    if far - near > 0.1 || face.part == FacePart::End {
                        let right = face.part == FacePart::Side && face.corners[0].1 == body.right;
                        each(&face, faces_camera(&face.corners, &projector, view), right);
                    }
                }
            }
        }
    }

    #[test]
    fn on_a_flat_straight_road_the_camera_sees_the_end_and_the_side_toward_it() {
        let view = RoadView::flat();
        for left_hand_traffic in [true, false] {
            let (mut sides, mut hidden_sides, mut ends) = (0, 0, 0);
            faces_seen(&view, left_hand_traffic, |face, seen, right| {
                match face.part {
                    FacePart::End => {
                        assert!(seen);
                        ends += 1;
                    }
                    FacePart::Side => {
                        // The left side is seen from the left of it, the right side from the right.
                        let lateral = face.corners[0].1;
                        let expected = if right {
                            CAMERA_LATERAL > lateral
                        } else {
                            CAMERA_LATERAL < lateral
                        };
                        assert_eq!(seen, expected, "{face:?}");
                        sides += usize::from(seen);
                        hidden_sides += usize::from(!seen);
                    }
                    // The roofs of cars and trucks are above the eye of the camera;
                    // only the tops of the wheels and the frames are below it.
                    FacePart::Roof => {
                        assert_eq!(seen, face.corners[0].2 < CAMERA_HEIGHT, "{face:?}")
                    }
                }
            });
            assert!(ends > 500 && sides > 500 && hidden_sides > 500);
        }
    }

    #[test]
    fn on_a_bend_the_camera_sees_the_side_of_a_car_in_its_own_lane() {
        // The road bends away to one side, so the cars in the own lane turn
        // one of their sides toward the camera; to the other side, the other.
        let own_lane_side = |bend: f32, left_hand_traffic: bool| {
            let view = RoadView::curved(bend, 0.0);
            let mut seen = [0usize; 2];
            faces_seen(&view, left_hand_traffic, |face, seen_now, right| {
                // The inner faces of the wheels, under the car, are seen from the
                // other side too: only the sides of the body count.
                let on_body = face.corners.iter().all(|c| c.2 >= 0.2);
                if face.part == FacePart::Side && face.lateral == -1.75 && on_body && seen_now {
                    seen[usize::from(right)] += 1;
                }
            });
            seen
        };
        for left_hand_traffic in [true, false] {
            let (toward, away) = (
                own_lane_side(30.0, left_hand_traffic),
                own_lane_side(-30.0, left_hand_traffic),
            );
            // A bend one way shows one side and never the other; the other bend shows it the other way.
            assert!(toward[1] > 20 && toward[0] == 0, "{toward:?}");
            assert!(away[0] > 20 && away[1] == 0, "{away:?}");
            assert_eq!(own_lane_side(0.0, left_hand_traffic), [0, 0]);
        }
    }

    #[test]
    fn where_the_road_rises_ahead_the_camera_sees_the_roofs_of_cars() {
        // At the bottom of a dip the road climbs: a car up the road is lifted
        // above the eye, but its roof plane tilts up so far that the eye lies above it.
        let view = RoadView::curved(0.0, 30.0);
        for left_hand_traffic in [true, false] {
            let (mut roofs, mut truck_roofs, mut all) = (0, 0, 0);
            faces_seen(&view, left_hand_traffic, |face, seen, _| {
                all += usize::from(face.part == FacePart::Roof);
                if face.part == FacePart::Roof && seen {
                    let height = face.corners[0].2;
                    roofs += 1;
                    truck_roofs += usize::from(height > 3.0);
                }
            });
            assert!(roofs > 100, "{roofs} of {all}");
            // A truck is high: its roof is seen from farther up the road only.
            assert!(truck_roofs < roofs);
        }
    }
}
