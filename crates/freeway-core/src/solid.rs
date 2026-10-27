//! Bodies that have the outline of a real car.
//!
//! A solid is its side view, an outline from one end to the other, pushed out
//! across the road: the hood, the windshield, the roof and the trunk are
//! strips of the outline that run across the car, and the sides follow the
//! outline from the shoulder line down to the underside. Above the shoulder
//! line the sides lean inward, so that the roof is narrower than the body.
//!
//! Every face is flat. A strip is a trapezoid with two edges across the car,
//! and a side lies on the plane through the lines it is cut from, so the
//! faces work as those of the boxes of `traffic::BodyBox` do: the scene keeps
//! those that turn toward the camera and decides which lights they cover.

use crate::traffic::FacePart;

/// Most points an outline has.
pub const MAX_OUTLINE: usize = 16;

/// A face as the distance, lateral position and height of its four corners.
pub type Quad = [(f32, f32, f32); 4];

/// A point of the outline as (distance, height).
type Point = (f32, f32);

/// What the surface of a part of the outline is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// Paint: the strip and the side below it.
    Body,
    /// Windshield or rear window: the strip is glass, and so is the side above
    /// the shoulder line.
    Glass,
    /// A roof: the strip is paint and the side above the shoulder line is glass.
    Cabin,
}

/// What a solid is made of, from its front end: the points of the upper
/// outline as (distance from the front end, height, surface of the part that
/// runs from the point to the next), and the rest of its shape.
pub struct SolidSpec {
    pub outline: &'static [(f32, f32, Surface)],
    pub half_width: f32,
    /// The height of the underside.
    pub bottom: f32,
    /// Up to this height the sides stand straight; above it they lean inward.
    pub shoulder: f32,
    /// How much narrower the roof is than the body on each side.
    pub inset: f32,
    /// How far the vertical corners of the front end are cut off.
    pub chamfer: f32,
}

/// A solid placed on the road. Distances grow along the outline, so the
/// first end is the one nearer to the camera.
#[derive(Clone, Copy, Debug)]
pub struct Solid {
    points: [(f32, f32); MAX_OUTLINE],
    surfaces: [Surface; MAX_OUTLINE],
    count: usize,
    bottom: f32,
    half_width: f32,
    shoulder: f32,
    inset: f32,
    /// The highest point of the outline.
    roof: f32,
    /// Where the front end is, how far its corners are cut off, and which way
    /// along the road the solid runs from it, `1.0` or `-1.0`.
    chamfer: (f32, f32, f32),
    /// Whether the first end is the end of the solid, not cut off.
    has_end: bool,
    lateral: f32,
}

/// Tolerance for lengths that are the same.
const EPS: f32 = 1e-3;

/// The distance and the height of the part of a segment at `d`.
fn along(a: (f32, f32), b: (f32, f32), d: f32) -> (f32, f32) {
    if (b.0 - a.0).abs() < 1e-6 {
        return (d, a.1);
    }
    (d, a.1 + (b.1 - a.1) * (d - a.0) / (b.0 - a.0))
}

impl SolidSpec {
    /// The solid with its front end `front` meters from the camera, for a car
    /// `length` meters long that is in lateral position `lateral`. Where
    /// `front_first` is not set, the rear end is the nearer one and the solid
    /// runs the other way. It is cut off before `near` and after `far`;
    /// `None` where nothing of it is left.
    pub fn place(
        &self,
        front: f32,
        length: f32,
        front_first: bool,
        lateral: f32,
        (near, far): (f32, f32),
    ) -> Option<Solid> {
        let count = self.outline.len();
        debug_assert!((2..=MAX_OUTLINE).contains(&count));
        let (mut points, mut surfaces) = ([(0.0, 0.0); MAX_OUTLINE], [Surface::Body; MAX_OUTLINE]);
        // The outline is given from the front; the end that faces the camera
        // comes first, and distances run away from the camera.
        for (index, &(x, z, _)) in self.outline.iter().enumerate() {
            if front_first {
                points[index] = (front + x, z);
            } else {
                let from_rear = count - 1 - index;
                points[from_rear] = (front + length - x, z);
            }
        }
        for index in 0..count - 1 {
            // The surface belongs to the part between two points.
            surfaces[if front_first {
                index
            } else {
                count - 2 - index
            }] = self.outline[index].2;
        }
        let first = points[0].0;
        let (mut kept, mut kept_surfaces, mut kept_count) =
            ([(0.0, 0.0); MAX_OUTLINE], [Surface::Body; MAX_OUTLINE], 0);
        for index in 0..count - 1 {
            let (a, b) = (points[index], points[index + 1]);
            if b.0 < near || a.0 >= far || (b.0 <= near && a.0 < b.0) {
                continue;
            }
            let from = if a.0 < near { along(a, b, near) } else { a };
            let to = if b.0 > far { along(a, b, far) } else { b };
            if kept_count == 0 {
                kept[0] = from;
                kept_count = 1;
            }
            if kept_count < MAX_OUTLINE {
                kept_surfaces[kept_count - 1] = surfaces[index];
                kept[kept_count] = to;
                kept_count += 1;
            }
        }
        if kept_count < 2 || kept[kept_count - 1].0 - kept[0].0 < EPS {
            return None;
        }
        let roof = kept[..kept_count]
            .iter()
            .map(|p| p.1)
            .fold(f32::MIN, f32::max);
        let (chamfer_front, direction) = if front_first {
            (front, 1.0)
        } else {
            (front + length, -1.0)
        };
        Some(Solid {
            points: kept,
            surfaces: kept_surfaces,
            count: kept_count,
            bottom: self.bottom,
            half_width: self.half_width,
            shoulder: self.shoulder,
            inset: self.inset,
            roof,
            chamfer: (chamfer_front, self.chamfer, direction),
            has_end: first >= near - EPS,
            lateral,
        })
    }
}

impl Solid {
    /// The outline.
    fn outline(&self) -> &[(f32, f32)] {
        &self.points[..self.count]
    }

    /// The half width of the body seen from above at distance `d`: less where
    /// the corners of the front end are cut off.
    fn plan(&self, d: f32) -> f32 {
        let (front, size, direction) = self.chamfer;
        self.half_width - (size - (d - front) * direction).clamp(0.0, size)
    }

    /// The half width at distance `d` and height `z`: less above the shoulder.
    fn width(&self, d: f32, z: f32) -> f32 {
        let rise = (z - self.shoulder).max(0.0);
        self.plan(d) - self.inset * (rise / (self.roof - self.shoulder).max(EPS)).min(1.0)
    }

    /// The extent of the solid: nearest and farthest distance, left and right
    /// lateral position, lowest and highest height.
    pub fn bounds(&self) -> (f32, f32, f32, f32, f32, f32) {
        let outline = self.outline();
        let (near, far) = (outline[0].0, outline[outline.len() - 1].0);
        let wide = outline
            .iter()
            .map(|&(d, _)| self.plan(d))
            .fold(0.0, f32::max);
        (
            near,
            far,
            self.lateral - wide,
            self.lateral + wide,
            self.bottom,
            self.roof,
        )
    }

    /// The highest height of the solid.
    pub fn top(&self) -> f32 {
        self.roof
    }

    /// Whether the end that faces the camera is the end of the solid.
    pub fn has_end(&self) -> bool {
        self.has_end
    }

    /// The points that cast the shadow of the solid, as (distance, lateral
    /// position, height): the outline at both sides and its feet on the road.
    pub fn casts(&self) -> Vec<(f32, f32, f32)> {
        let mut points = Vec::with_capacity(self.count * 4);
        for &(d, z) in self.outline() {
            let (high, low) = (self.width(d, z), self.plan(d));
            points.push((d, self.lateral - high, z));
            points.push((d, self.lateral + high, z));
            points.push((d, self.lateral - low, 0.0));
            points.push((d, self.lateral + low, 0.0));
        }
        points
    }

    /// Whether the point, given as (distance, lateral position, height), is
    /// inside the solid.
    pub fn contains(&self, d: f32, lateral: f32, height: f32) -> bool {
        let outline = self.outline();
        let upper = outline.windows(2).find_map(|w| {
            let (a, b) = (w[0], w[1]);
            (d >= a.0 && d <= b.0 && b.0 - a.0 > 1e-6).then(|| along(a, b, d).1)
        });
        upper.is_some_and(|upper| {
            height > self.bottom
                && height < upper
                && (lateral - self.lateral).abs() < self.width(d, height)
        })
    }

    /// The flat faces of the solid, each with the corners running the way that
    /// turns the face toward the camera when seen from outside it.
    pub fn faces(&self) -> Vec<(FacePart, Quad)> {
        let centre = self.lateral;
        let outline = self.outline();
        let mut out = Vec::with_capacity(4 * self.count);
        if self.has_end {
            let (d, z) = outline[0];
            let (low, high) = (self.plan(d), self.width(d, z));
            if z - self.bottom > EPS {
                out.push((
                    FacePart::End,
                    [
                        (d, centre - low, self.bottom),
                        (d, centre + low, self.bottom),
                        (d, centre + high, z),
                        (d, centre - high, z),
                    ],
                ));
            }
        }
        // The parts of the outline, cut where they cross the shoulder line, so
        // that each is wholly below or wholly above it.
        let mut parts: Vec<(Point, Point, Surface)> = Vec::with_capacity(self.count + 4);
        for index in 0..self.count - 1 {
            let (a, b) = (outline[index], outline[index + 1]);
            let surface = self.surfaces[index];
            let across = (a.1 < self.shoulder - EPS && b.1 > self.shoulder + EPS)
                || (a.1 > self.shoulder + EPS && b.1 < self.shoulder - EPS);
            if across {
                let t = (self.shoulder - a.1) / (b.1 - a.1);
                let middle = (a.0 + (b.0 - a.0) * t, self.shoulder);
                parts.push((a, middle, surface));
                parts.push((middle, b, surface));
            } else {
                parts.push((a, b, surface));
            }
        }
        // A run of parts that all stand on the shoulder line has one flat side.
        let mut run: Option<(f32, f32)> = None;
        for &(a, b, surface) in &parts {
            let length = b.0 - a.0;
            let (wa, wb) = (self.width(a.0, a.1), self.width(b.0, b.1));
            // The strip across the car.
            if length > EPS || (b.1 - a.1).abs() > EPS {
                let (rise, run_d) = (b.1 - a.1, length);
                let part = if surface == Surface::Glass {
                    FacePart::Glass
                } else if run_d.abs() >= rise.abs() {
                    FacePart::Roof
                } else {
                    FacePart::End
                };
                out.push((
                    part,
                    [
                        (a.0, centre - wa, a.1),
                        (a.0, centre + wa, a.1),
                        (b.0, centre + wb, b.1),
                        (b.0, centre - wb, b.1),
                    ],
                ));
            }
            if length <= EPS {
                continue;
            }
            let (pa, pb) = (self.plan(a.0), self.plan(b.0));
            let (la, lb) = (a.1.min(self.shoulder), b.1.min(self.shoulder));
            let flat = la >= self.shoulder - EPS
                && lb >= self.shoulder - EPS
                && (pa - self.half_width).abs() < EPS
                && (pb - self.half_width).abs() < EPS;
            if flat {
                run = Some(match run {
                    Some((from, _)) => (from, b.0),
                    None => (a.0, b.0),
                });
            } else {
                if let Some((from, to)) = run.take() {
                    self.push_flat_sides(&mut out, from, to);
                }
                if la.max(lb) - self.bottom > EPS {
                    self.push_sides(&mut out, (a.0, pa, la), (b.0, pb, lb), FacePart::Side);
                }
            }
            // Above the shoulder line the sides lean in.
            if a.1.min(b.1) >= self.shoulder - EPS && a.1.max(b.1) > self.shoulder + EPS {
                let part = if surface == Surface::Body {
                    FacePart::Side
                } else {
                    FacePart::Glass
                };
                self.push_leaning_sides(&mut out, (a.0, a.1, wa, pa), (b.0, b.1, wb, pb), part);
            }
        }
        if let Some((from, to)) = run.take() {
            self.push_flat_sides(&mut out, from, to);
        }
        out
    }

    /// The sides under a stretch of the outline that is at the shoulder line
    /// or above it: flat rectangles.
    fn push_flat_sides(&self, out: &mut Vec<(FacePart, Quad)>, from: f32, to: f32) {
        let (a, b) = (
            (from, self.half_width, self.shoulder),
            (to, self.half_width, self.shoulder),
        );
        self.push_sides(out, a, b, FacePart::Side);
    }

    /// The two sides between `a` and `b`, each as (distance, half width,
    /// height of the upper edge), from the underside up.
    fn push_sides(
        &self,
        out: &mut Vec<(FacePart, Quad)>,
        a: (f32, f32, f32),
        b: (f32, f32, f32),
        part: FacePart,
    ) {
        let centre = self.lateral;
        out.push((
            part,
            [
                (a.0, centre + a.1, self.bottom),
                (b.0, centre + b.1, self.bottom),
                (b.0, centre + b.1, b.2),
                (a.0, centre + a.1, a.2),
            ],
        ));
        out.push((
            part,
            [
                (a.0, centre - a.1, self.bottom),
                (a.0, centre - a.1, a.2),
                (b.0, centre - b.1, b.2),
                (b.0, centre - b.1, self.bottom),
            ],
        ));
    }

    /// The sides above the shoulder line between `a` and `b`, as (distance,
    /// height of the upper edge, half width there, half width at the
    /// shoulder line).
    fn push_leaning_sides(
        &self,
        out: &mut Vec<(FacePart, Quad)>,
        a: (f32, f32, f32, f32),
        b: (f32, f32, f32, f32),
        part: FacePart,
    ) {
        let centre = self.lateral;
        let shoulder = self.shoulder;
        out.push((
            part,
            [
                (a.0, centre + a.3, shoulder),
                (b.0, centre + b.3, shoulder),
                (b.0, centre + b.2, b.1),
                (a.0, centre + a.2, a.1),
            ],
        ));
        out.push((
            part,
            [
                (a.0, centre - a.3, shoulder),
                (a.0, centre - a.2, a.1),
                (b.0, centre - b.2, b.1),
                (b.0, centre - b.3, shoulder),
            ],
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Surface = Surface::Body;

    /// A convex solid: a hood, a windshield, a roof and a tail, 4 meters long.
    const SPEC: SolidSpec = SolidSpec {
        outline: &[
            (0.0, 0.6, B),
            (0.2, 0.8, B),
            (1.0, 0.9, Surface::Glass),
            (1.8, 1.4, Surface::Cabin),
            (3.0, 1.4, B),
            (4.0, 0.7, B),
        ],
        half_width: 0.9,
        bottom: 0.2,
        shoulder: 0.9,
        inset: 0.2,
        chamfer: 0.0,
    };

    fn centre(faces: &[(FacePart, Quad)]) -> (f32, f32, f32) {
        let mut sum = (0.0, 0.0, 0.0);
        let mut count = 0.0;
        for (_, quad) in faces {
            for c in quad {
                sum = (sum.0 + c.0, sum.1 + c.1, sum.2 + c.2);
                count += 1.0;
            }
        }
        (sum.0 / count, sum.1 / count, sum.2 / count)
    }

    #[test]
    fn every_face_turns_its_outside_to_the_side_it_looks_from() {
        // The faces run so that the cross product of two edges points into the
        // solid; seen from the outside they turn toward the camera.
        for front_first in [true, false] {
            let solid = SPEC
                .place(50.0, 4.0, front_first, 1.75, (1.0, 600.0))
                .unwrap();
            let faces = solid.faces();
            let middle = centre(&faces);
            for (part, c) in &faces {
                let cross = |a: (f32, f32, f32), b: (f32, f32, f32)| {
                    (
                        a.1 * b.2 - a.2 * b.1,
                        a.2 * b.0 - a.0 * b.2,
                        a.0 * b.1 - a.1 * b.0,
                    )
                };
                let edge = |i: usize, j: usize| (c[j].0 - c[i].0, c[j].1 - c[i].1, c[j].2 - c[i].2);
                // The corners run in turn, so the turn at the corner with the
                // widest angle tells which way: take the largest of the turns.
                let inward = [(0, 1, 2), (1, 2, 3), (2, 3, 0), (3, 0, 1)]
                    .map(|(i, j, k)| cross(edge(i, j), edge(j, k)))
                    .into_iter()
                    .max_by(|a, b| {
                        let size = |v: (f32, f32, f32)| v.0 * v.0 + v.1 * v.1 + v.2 * v.2;
                        size(*a).total_cmp(&size(*b))
                    })
                    .unwrap();
                let at = (
                    c.iter().map(|p| p.0).sum::<f32>() / 4.0 - middle.0,
                    c.iter().map(|p| p.1).sum::<f32>() / 4.0 - middle.1,
                    c.iter().map(|p| p.2).sum::<f32>() / 4.0 - middle.2,
                );
                let outward = -(inward.0 * at.0 + inward.1 * at.1 + inward.2 * at.2);
                assert!(outward > 0.0, "{part:?} {c:?}");
            }
        }
    }

    #[test]
    fn the_end_that_faces_the_camera_is_first_and_the_far_end_has_no_face() {
        for (front_first, height) in [(true, 0.6), (false, 0.7)] {
            let solid = SPEC
                .place(50.0, 4.0, front_first, 1.75, (1.0, 600.0))
                .unwrap();
            assert!(solid.has_end());
            let ends: Vec<_> = solid
                .faces()
                .into_iter()
                .filter(|(part, _)| *part == FacePart::End)
                .collect();
            assert_eq!(ends.len(), 1);
            assert!(ends[0].1.iter().all(|c| c.0 == 50.0));
            let top = ends[0].1.iter().map(|c| c.2).fold(f32::MIN, f32::max);
            assert!((top - height).abs() < 1e-5, "{top}");
        }
    }

    #[test]
    fn a_solid_that_is_cut_off_has_no_end_and_keeps_its_shape() {
        let whole = SPEC.place(50.0, 4.0, true, 0.0, (1.0, 600.0)).unwrap();
        let cut = SPEC.place(50.0, 4.0, true, 0.0, (51.0, 600.0)).unwrap();
        assert!(!cut.has_end());
        let (near, far, ..) = cut.bounds();
        assert_eq!((near, far), (51.0, 54.0));
        // The roof is where it was, and the part cut off at the near end is lower.
        assert!(cut.contains(52.9, 0.0, 1.3));
        assert!(whole.contains(52.9, 0.0, 1.3));
        assert!(!cut.contains(50.5, 0.0, 0.7));
        // Cut at the far end.
        let short = SPEC.place(50.0, 4.0, true, 0.0, (1.0, 52.0)).unwrap();
        assert_eq!(short.bounds().1, 52.0);
        // Nothing is left of a solid that is wholly cut off.
        assert!(SPEC.place(50.0, 4.0, true, 0.0, (60.0, 600.0)).is_none());
        assert!(SPEC.place(50.0, 4.0, true, 0.0, (1.0, 49.0)).is_none());
        // The outline is the same seen from the other side, cut by the camera.
        let rear_first = SPEC.place(50.0, 4.0, false, 0.0, (52.0, 600.0)).unwrap();
        assert!(!rear_first.has_end());
        assert_eq!(rear_first.bounds().0, 52.0);
    }

    #[test]
    fn above_the_shoulder_the_sides_lean_in_and_the_front_corners_are_cut() {
        let spec = SolidSpec {
            chamfer: 0.3,
            ..SPEC
        };
        let solid = spec.place(50.0, 4.0, true, 0.0, (1.0, 600.0)).unwrap();
        // At the shoulder line the body is as wide as it is, and the roof is narrower.
        assert!(solid.contains(51.5, 0.89, 0.85));
        assert!(!solid.contains(51.5, 0.91, 0.85));
        assert!(solid.contains(52.4, 0.0, 1.35));
        assert!(!solid.contains(52.4, 0.8, 1.35));
        assert!(solid.contains(52.4, 0.69, 1.35));
        // The corners of the front are cut: narrower there than further back.
        assert!(!solid.contains(50.05, 0.85, 0.65));
        assert!(solid.contains(50.4, 0.85, 0.8));
        // The faces are flat and finite.
        for (_, quad) in solid.faces() {
            assert!(
                quad.iter()
                    .all(|c| c.0.is_finite() && c.1.is_finite() && c.2.is_finite())
            );
        }
    }

    #[test]
    fn the_shadow_points_cover_the_outline_and_the_feet() {
        let solid = SPEC.place(50.0, 4.0, true, 0.0, (1.0, 600.0)).unwrap();
        let points = solid.casts();
        assert!(points.iter().any(|p| p.2 == 0.0));
        assert!(points.iter().any(|p| (p.2 - 1.4).abs() < 1e-5));
        assert!(points.iter().all(|p| (50.0..=54.0).contains(&p.0)));
        assert_eq!(solid.top(), 1.4);
    }
}
