//! Platform-independent decisions about user input.

/// Decides when mouse movement should end the screensaver.
///
/// Windows sends a mouse-move message right after a window appears even if
/// the mouse has not moved, so movement is measured from the first position
/// reported instead of ending on the first message.
#[derive(Debug, Clone)]
pub struct MouseExitTracker {
    origin: Option<(i32, i32)>,
    threshold: u32,
}

impl MouseExitTracker {
    /// `threshold` is the distance in pixels the cursor may move on either
    /// axis without ending the screensaver.
    pub const fn new(threshold: u32) -> Self {
        Self {
            origin: None,
            threshold,
        }
    }

    /// Records a cursor position in screen coordinates and returns whether
    /// the cursor has moved farther than the threshold from the first one.
    pub fn should_exit(&mut self, x: i32, y: i32) -> bool {
        let (ox, oy) = *self.origin.get_or_insert((x, y));
        x.abs_diff(ox) > self.threshold || y.abs_diff(oy) > self.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_position_never_exits() {
        let mut tracker = MouseExitTracker::new(10);
        assert!(!tracker.should_exit(500, 300));
    }

    #[test]
    fn small_jitter_does_not_exit() {
        let mut tracker = MouseExitTracker::new(10);
        tracker.should_exit(500, 300);
        assert!(!tracker.should_exit(510, 290));
        assert!(!tracker.should_exit(495, 305));
    }

    #[test]
    fn movement_beyond_threshold_on_either_axis_exits() {
        let mut tracker = MouseExitTracker::new(10);
        tracker.should_exit(500, 300);
        assert!(tracker.should_exit(511, 300));

        let mut tracker = MouseExitTracker::new(10);
        tracker.should_exit(500, 300);
        assert!(tracker.should_exit(500, 289));
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let mut tracker = MouseExitTracker::new(10);
        tracker.should_exit(i32::MIN, i32::MAX);
        assert!(tracker.should_exit(i32::MAX, i32::MIN));
    }
}
