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
}
