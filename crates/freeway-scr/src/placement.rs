//! Where the windows of a run are, and how large the picture of the several
//! monitors is when they are drawn as one screen
//! (docs/adr/0008-span-monitors-as-one-scene.md).

/// A rectangle of the screen in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    pub fn height(&self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }
}

/// The size of the area of the picture that the window has by default, in
/// pixels at 96 DPI.
pub const DEFAULT_CLIENT: (i32, i32) = (1280, 720);

/// The window never leaves out less than this of the work area: its picture
/// is at most this part of the work area (in percent), so that the window
/// does not look as if it were maximized.
const LARGEST_SHARE_PERCENT: i64 = 90;

/// The smallest picture, so that a tiny work area does not give an empty window.
const SMALLEST_CLIENT: (i32, i32) = (320, 180);

/// The outer rectangle of the window that opens on a screen whose usable
/// area is `work`, at `dpi` dots per inch.
///
/// `frame` is how much wider and taller than its picture the window is (the
/// border and the title bar at that DPI). The picture is 1280 by 720 pixels
/// at 96 DPI, grows with the DPI, and shrinks, keeping its proportions, until
/// it takes at most 90% of the work area. The window is centered in the work
/// area.
pub fn window_rect(work: Rect, dpi: u32, frame: (i32, i32)) -> Rect {
    let dpi = i64::from(if dpi == 0 { 96 } else { dpi });
    let wanted = (
        i64::from(DEFAULT_CLIENT.0) * dpi / 96,
        i64::from(DEFAULT_CLIENT.1) * dpi / 96,
    );
    let room = |work: i32, frame: i32| {
        (i64::from(work) - i64::from(frame)).max(0) * LARGEST_SHARE_PERCENT / 100
    };
    let (room_width, room_height) = (room(work.width(), frame.0), room(work.height(), frame.1));
    // Shrink by the tighter of the two limits; the proportions stay.
    let mut client = wanted;
    if room_width < client.0 || room_height < client.1 {
        if room_width * wanted.1 < room_height * wanted.0 {
            client = (room_width, room_width * wanted.1 / wanted.0);
        } else {
            client = (room_height * wanted.0 / wanted.1, room_height);
        }
    }
    let smallest = (
        i64::from(SMALLEST_CLIENT.0) * dpi / 96,
        i64::from(SMALLEST_CLIENT.1) * dpi / 96,
    );
    let client = (client.0.max(smallest.0), client.1.max(smallest.1));
    // At most the size of an `i32`, which a screen never exceeds.
    let width = i32::try_from(client.0 + i64::from(frame.0)).unwrap_or(i32::MAX);
    let height = i32::try_from(client.1 + i64::from(frame.1)).unwrap_or(i32::MAX);
    // Never beyond the top left corner of the work area.
    let left = work.left + (work.width() - width).max(0) / 2;
    let top = work.top + (work.height() - height).max(0) / 2;
    Rect {
        left,
        top,
        right: left.saturating_add(width),
        bottom: top.saturating_add(height),
    }
}

/// The monitors drawn as one screen: the picture that covers all of them, and
/// where each monitor is in it. Everything is in physical pixels, so that
/// monitors of different sizes and DPI line up the way Windows lays them out.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanLayout {
    /// The size of the rectangle around all the monitors.
    pub width: f32,
    pub height: f32,
    /// The focal length of the projection of that picture (see
    /// `freeway_core::projection::focal_length`).
    pub focal: f32,
    /// The vanishing point: the middle of the primary monitor, in the
    /// coordinates of the picture, whose origin is the top left corner of the
    /// rectangle around all the monitors.
    pub center: (f32, f32),
    /// The rectangle of each monitor, in the same order as the monitors given,
    /// in the coordinates of the picture. The window of a monitor draws this
    /// part of the picture, so its top left corner is how far the picture is
    /// shifted for that window.
    pub parts: Vec<Rect>,
}

/// Lays out `monitors` (their rectangles on the virtual desktop) as one
/// screen, with the vanishing point in the middle of `monitors[primary]`.
/// `None` when there is no monitor, `primary` is not one of them, or a
/// rectangle is empty or so large that it would not be computed exactly.
pub fn span_layout(monitors: &[Rect], primary: usize) -> Option<SpanLayout> {
    // Beyond this a coordinate does not stay exact as an `f32` pixel.
    const LIMIT: i64 = 1 << 24;
    let primary = monitors.get(primary)?;
    if monitors.iter().any(|m| m.width() <= 0 || m.height() <= 0) {
        return None;
    }
    let left = monitors.iter().map(|m| i64::from(m.left)).min()?;
    let top = monitors.iter().map(|m| i64::from(m.top)).min()?;
    let right = monitors.iter().map(|m| i64::from(m.right)).max()?;
    let bottom = monitors.iter().map(|m| i64::from(m.bottom)).max()?;
    if right - left > LIMIT || bottom - top > LIMIT {
        return None;
    }
    // In the picture's own coordinates, whose origin is (left, top).
    let to_picture = |m: &Rect| Rect {
        left: (i64::from(m.left) - left) as i32,
        top: (i64::from(m.top) - top) as i32,
        right: (i64::from(m.right) - left) as i32,
        bottom: (i64::from(m.bottom) - top) as i32,
    };
    let (width, height) = ((right - left) as f32, (bottom - top) as f32);
    let primary = to_picture(primary);
    Some(SpanLayout {
        width,
        height,
        focal: freeway_core::projection::focal_length(width, height),
        center: (
            (primary.left + primary.right) as f32 * 0.5,
            (primary.top + primary.bottom) as f32 * 0.5,
        ),
        parts: monitors.iter().map(to_picture).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME_96: (i32, i32) = (16, 39);

    fn work(width: i32, height: i32) -> Rect {
        Rect {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    }

    fn client(rect: Rect, frame: (i32, i32)) -> (i32, i32) {
        (rect.width() - frame.0, rect.height() - frame.1)
    }

    #[test]
    fn the_default_picture_is_1280_by_720_in_the_middle() {
        let rect = window_rect(work(1920, 1040), 96, FRAME_96);
        assert_eq!(client(rect, FRAME_96), (1280, 720));
        assert_eq!((rect.left, rect.top), ((1920 - 1296) / 2, (1040 - 759) / 2));
    }

    #[test]
    fn the_picture_grows_with_the_dpi_when_it_fits() {
        let frame = (24, 58);
        let rect = window_rect(work(3840, 2120), 144, frame);
        assert_eq!(client(rect, frame), (1920, 1080));
    }

    #[test]
    fn the_picture_shrinks_to_fit_and_keeps_its_proportions() {
        for (size, dpi, frame) in [
            ((1366, 728), 96, FRAME_96),
            ((1920, 1040), 144, (24, 58)),
            ((800, 560), 96, FRAME_96),
            ((1000, 2000), 96, FRAME_96),
        ] {
            let rect = window_rect(work(size.0, size.1), dpi, frame);
            let (width, height) = client(rect, frame);
            // The same proportions, within a pixel of rounding.
            assert!(
                (width * 9 - height * 16).abs() <= 16,
                "{size:?} {width} {height}"
            );
            // Inside the work area, with the window at most 90% of it plus the frame.
            assert!(rect.left >= 0 && rect.top >= 0, "{size:?}");
            assert!(
                rect.right <= size.0 && rect.bottom <= size.1,
                "{size:?} {rect:?}"
            );
            assert!(
                i64::from(width) <= i64::from(size.0 - frame.0) * 90 / 100,
                "{size:?}"
            );
            assert!(
                i64::from(height) <= i64::from(size.1 - frame.1) * 90 / 100,
                "{size:?}"
            );
        }
    }

    #[test]
    fn the_window_is_centered_in_a_work_area_that_does_not_start_at_the_origin() {
        // A second monitor on the left, and a taskbar at the top.
        let area = Rect {
            left: -1920,
            top: 40,
            right: 0,
            bottom: 1080,
        };
        let rect = window_rect(area, 96, FRAME_96);
        // Centered, within a pixel of rounding.
        assert!((rect.left - area.left - (area.right - rect.right)).abs() <= 1);
        assert!((rect.top - area.top - (area.bottom - rect.bottom)).abs() <= 1);
    }

    #[test]
    fn a_tiny_or_empty_work_area_still_gives_a_window() {
        for size in [(0, 0), (100, 100), (-5, -5)] {
            let rect = window_rect(work(size.0, size.1), 96, FRAME_96);
            assert!(rect.width() >= 320 && rect.height() >= 180, "{size:?}");
            assert_eq!((rect.left, rect.top), (0, 0), "{size:?}");
        }
        // An unknown DPI counts as 96.
        assert_eq!(
            window_rect(work(1920, 1040), 0, FRAME_96),
            window_rect(work(1920, 1040), 96, FRAME_96)
        );
    }

    fn monitor(left: i32, top: i32, width: i32, height: i32) -> Rect {
        Rect {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    #[test]
    fn one_monitor_is_the_picture_with_the_vanishing_point_in_its_middle() {
        // Wherever the desktop puts it.
        for origin in [(0, 0), (-1920, 0), (0, -1080), (1920, 300)] {
            let layout = span_layout(&[monitor(origin.0, origin.1, 1920, 1080)], 0).unwrap();
            assert_eq!((layout.width, layout.height), (1920.0, 1080.0));
            assert_eq!(layout.center, (960.0, 540.0));
            assert_eq!(layout.parts, [monitor(0, 0, 1920, 1080)]);
            assert_eq!(
                layout.focal,
                freeway_core::projection::focal_length(1920.0, 1080.0)
            );
        }
    }

    #[test]
    fn the_vanishing_point_is_in_the_middle_of_the_primary_monitor() {
        let (a, b, c) = (
            monitor(-1920, 0, 1920, 1080),
            monitor(0, 0, 1920, 1080),
            monitor(1920, 0, 1920, 1080),
        );
        // The primary monitor on the left, in the middle and on the right.
        for (monitors, primary, center_x) in [
            ([b, c, monitor(3840, 0, 1920, 1080)], 0, 960.0),
            ([a, b, c], 1, 1920.0 + 960.0),
            ([a, b, monitor(1920, 0, 1920, 1080)], 2, 3840.0 + 960.0),
        ] {
            let layout = span_layout(&monitors, primary).unwrap();
            assert_eq!((layout.width, layout.height), (5760.0, 1080.0), "{primary}");
            assert_eq!(layout.center, (center_x, 540.0), "{primary}");
            // The focal length follows the whole picture, not the primary monitor.
            assert_eq!(layout.focal, 2880.0, "{primary}");
        }
    }

    #[test]
    fn the_parts_tile_the_picture_where_the_monitors_touch() {
        let layout = span_layout(
            &[
                monitor(0, 0, 1920, 1080),
                monitor(1920, 0, 1920, 1080),
                monitor(-1920, 0, 1920, 1080),
            ],
            0,
        )
        .unwrap();
        // The monitor that was left of the origin starts the picture.
        assert_eq!(
            layout.parts,
            [
                monitor(1920, 0, 1920, 1080),
                monitor(3840, 0, 1920, 1080),
                monitor(0, 0, 1920, 1080)
            ]
        );
        assert_eq!(layout.center, (1920.0 + 960.0, 540.0));
        // Neighbours meet at one pixel column: the same point of the picture
        // is the edge of both windows.
        assert_eq!(layout.parts[2].right, layout.parts[0].left);
        assert_eq!(layout.parts[0].right, layout.parts[1].left);
    }

    #[test]
    fn monitors_above_and_below_each_other() {
        let layout = span_layout(
            &[monitor(0, 0, 1920, 1080), monitor(0, 1080, 1920, 1080)],
            1,
        )
        .unwrap();
        assert_eq!((layout.width, layout.height), (1920.0, 2160.0));
        assert_eq!(layout.center, (960.0, 1080.0 + 540.0));
        // The taller picture sets the focal length.
        assert_eq!(layout.focal, 2160.0 * 0.55);
        assert_eq!(layout.parts[1], monitor(0, 1080, 1920, 1080));
        // A monitor above the primary one: the vanishing point is lower.
        let above = span_layout(
            &[monitor(0, -1080, 1920, 1080), monitor(0, 0, 1920, 1080)],
            1,
        )
        .unwrap();
        assert_eq!(above.center, (960.0, 1080.0 + 540.0));
        assert_eq!(above.parts[0], monitor(0, 0, 1920, 1080));
    }

    #[test]
    fn monitors_of_different_sizes_and_dpi_line_up_in_physical_pixels() {
        // A 4K monitor at 150% beside a 1080p monitor at 100%, tops aligned
        // the way Windows lays out the physical desktop, and the small one
        // lower than the top of the big one.
        let layout = span_layout(
            &[monitor(0, 0, 3840, 2160), monitor(3840, 400, 1920, 1080)],
            0,
        )
        .unwrap();
        assert_eq!((layout.width, layout.height), (5760.0, 2160.0));
        assert_eq!(layout.center, (1920.0, 1080.0));
        assert_eq!(layout.parts[1], monitor(3840, 400, 1920, 1080));
        assert_eq!(layout.focal, 2880.0);
        // A point in the overlap of both windows' rows is the same point of the picture.
        assert!(layout.parts[1].top >= layout.parts[0].top);
        assert!(layout.parts[1].bottom <= layout.parts[0].bottom);
    }

    #[test]
    fn an_l_shaped_layout_has_parts_only_where_there_are_monitors() {
        // Two monitors side by side and one below the left one.
        let monitors = [
            monitor(0, 0, 1920, 1080),
            monitor(1920, 0, 1920, 1080),
            monitor(0, 1080, 1920, 1080),
        ];
        let layout = span_layout(&monitors, 1).unwrap();
        assert_eq!((layout.width, layout.height), (3840.0, 2160.0));
        assert_eq!(layout.center, (1920.0 + 960.0, 540.0));
        assert_eq!(layout.parts, monitors);
        // The bottom right corner of the picture belongs to no part.
        let covered = |x: i32, y: i32| {
            layout
                .parts
                .iter()
                .any(|p| p.left <= x && x < p.right && p.top <= y && y < p.bottom)
        };
        assert!(covered(100, 2000) && covered(3000, 100));
        assert!(!covered(3000, 2000));
    }

    #[test]
    fn a_layout_that_cannot_be_made_is_none() {
        let one = monitor(0, 0, 1920, 1080);
        assert!(span_layout(&[], 0).is_none());
        assert!(span_layout(&[one], 1).is_none());
        assert!(span_layout(&[monitor(0, 0, 0, 1080)], 0).is_none());
        assert!(span_layout(&[one, monitor(100, 100, -5, 50)], 0).is_none());
        // Far apart: not exact in pixels of the picture.
        assert!(span_layout(&[one, monitor(i32::MAX - 1920, 0, 1920, 1080)], 0).is_none());
        assert!(
            span_layout(
                &[
                    monitor(i32::MIN, 0, 10, 10),
                    monitor(i32::MAX - 10, 0, 10, 10)
                ],
                0
            )
            .is_none()
        );
    }
}
