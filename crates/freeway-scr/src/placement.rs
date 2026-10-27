//! Where and how large the window of the `/window` mode opens.

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
}
