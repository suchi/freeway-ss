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

/// A key that a run of the `/window` or `/play` mode reacts to. Every other
/// key is `Other`; it still counts toward the secret sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Space,
    D,
    F,
    A,
    B,
    Other,
}

/// What a key press asks the run to do once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    TogglePause,
    ToggleSide,
    ToggleFullscreen,
    CallUfo,
}

/// The secret sequence: up, up, down, down, left, right, left, right, B, A.
const SEQUENCE: [Key; 10] = [
    Key::Up,
    Key::Up,
    Key::Down,
    Key::Down,
    Key::Left,
    Key::Right,
    Key::Left,
    Key::Right,
    Key::B,
    Key::A,
];

/// Which keys are held, and the sequence of the last keys pressed.
#[derive(Debug, Default)]
pub struct Controls {
    up: bool,
    down: bool,
    /// The last ten keys that were pressed, the oldest first.
    recent: Vec<Key>,
}

impl Controls {
    /// A key went down. `repeat` is set where the key was already held and
    /// the system repeats it: a held key neither toggles again nor counts as
    /// another press of the sequence. Returns what the key asks for.
    pub fn key_down(&mut self, key: Key, repeat: bool) -> Option<Command> {
        match key {
            Key::Up => self.up = true,
            Key::Down => self.down = true,
            _ => {}
        }
        if repeat {
            return None;
        }
        self.recent.push(key);
        if self.recent.len() > SEQUENCE.len() {
            self.recent.remove(0);
        }
        if self.recent == SEQUENCE {
            self.recent.clear();
            return Some(Command::CallUfo);
        }
        match key {
            Key::Space => Some(Command::TogglePause),
            Key::D => Some(Command::ToggleSide),
            Key::F => Some(Command::ToggleFullscreen),
            _ => None,
        }
    }

    pub fn key_up(&mut self, key: Key) {
        match key {
            Key::Up => self.up = false,
            Key::Down => self.down = false,
            _ => {}
        }
    }

    /// The window lost the keyboard: no key is held any more, because the
    /// release will not be seen.
    pub fn release_all(&mut self) {
        self.up = false;
        self.down = false;
    }

    /// `1.0` while only up is held, `-1.0` while only down is held, and `0.0`
    /// otherwise.
    pub fn speed_direction(&self) -> f32 {
        f32::from(i8::from(self.up) - i8::from(self.down))
    }
}

/// How long the hint stays at full strength after the last input, and how
/// long it takes to fade out, in seconds.
pub const HINT_SECONDS: f32 = 4.0;
pub const HINT_FADE_SECONDS: f32 = 0.5;

/// When the hint about the keys is shown: it begins shown, goes out after
/// some time without input, and comes back with the next input.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hint {
    idle: f32,
}

impl Hint {
    /// Input happened: show the hint in full again.
    pub fn touch(&mut self) {
        self.idle = 0.0;
    }

    /// `dt` seconds passed without input.
    pub fn update(&mut self, dt: f32) {
        if dt.is_finite() && dt > 0.0 {
            self.idle = (self.idle + dt).min(HINT_SECONDS + HINT_FADE_SECONDS);
        }
    }

    /// How strong the hint is, from 0 (gone) to 1. While time stands still
    /// the hint stays, to show that it does.
    pub fn opacity(&self, paused: bool) -> f32 {
        if paused {
            return 1.0;
        }
        (1.0 - (self.idle - HINT_SECONDS) / HINT_FADE_SECONDS).clamp(0.0, 1.0)
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

    /// Presses `keys` one after the other, each once, and returns the command
    /// of the last one.
    fn press(controls: &mut Controls, keys: &[Key]) -> Option<Command> {
        let mut last = None;
        for &key in keys {
            last = controls.key_down(key, false);
            controls.key_up(key);
        }
        last
    }

    #[test]
    fn the_keys_ask_for_their_commands() {
        let mut controls = Controls::default();
        assert_eq!(
            controls.key_down(Key::Space, false),
            Some(Command::TogglePause)
        );
        assert_eq!(controls.key_down(Key::D, false), Some(Command::ToggleSide));
        assert_eq!(
            controls.key_down(Key::F, false),
            Some(Command::ToggleFullscreen)
        );
        for key in [
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
            Key::A,
            Key::B,
            Key::Other,
        ] {
            assert_eq!(controls.key_down(key, false), None, "{key:?}");
        }
    }

    #[test]
    fn a_held_key_does_not_toggle_again() {
        let mut controls = Controls::default();
        assert!(controls.key_down(Key::Space, false).is_some());
        for _ in 0..10 {
            assert_eq!(controls.key_down(Key::Space, true), None);
        }
        assert!(controls.key_down(Key::D, false).is_some());
        assert_eq!(controls.key_down(Key::D, true), None);
    }

    #[test]
    fn the_speed_keys_give_a_direction_while_they_are_held() {
        let mut controls = Controls::default();
        assert_eq!(controls.speed_direction(), 0.0);
        controls.key_down(Key::Up, false);
        controls.key_down(Key::Up, true);
        assert_eq!(controls.speed_direction(), 1.0);
        controls.key_down(Key::Down, false);
        assert_eq!(controls.speed_direction(), 0.0);
        controls.key_up(Key::Up);
        assert_eq!(controls.speed_direction(), -1.0);
        controls.key_up(Key::Down);
        assert_eq!(controls.speed_direction(), 0.0);
    }

    #[test]
    fn losing_the_keyboard_releases_the_speed_keys() {
        let mut controls = Controls::default();
        controls.key_down(Key::Up, false);
        controls.release_all();
        assert_eq!(controls.speed_direction(), 0.0);
    }

    const SECRET: [Key; 10] = [
        Key::Up,
        Key::Up,
        Key::Down,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Left,
        Key::Right,
        Key::B,
        Key::A,
    ];

    #[test]
    fn the_sequence_calls_the_ufo_at_its_last_key_only() {
        let mut controls = Controls::default();
        assert_eq!(press(&mut controls, &SECRET[..9]), None);
        assert_eq!(controls.key_down(Key::A, false), Some(Command::CallUfo));
        // It starts over: the same keys call it again.
        assert_eq!(press(&mut controls, &SECRET), Some(Command::CallUfo));
    }

    #[test]
    fn keys_before_the_sequence_do_not_matter() {
        let mut controls = Controls::default();
        press(
            &mut controls,
            &[Key::Other, Key::F, Key::Up, Key::Left, Key::B],
        );
        assert_eq!(press(&mut controls, &SECRET), Some(Command::CallUfo));
    }

    #[test]
    fn a_wrong_key_inside_the_sequence_breaks_it() {
        for at in 0..9 {
            let mut controls = Controls::default();
            let mut keys = SECRET.to_vec();
            keys[at] = Key::Other;
            assert_eq!(press(&mut controls, &keys), None, "{at}");
        }
        // An extra key in the middle breaks it as well.
        let mut controls = Controls::default();
        press(&mut controls, &SECRET[..5]);
        press(&mut controls, &[Key::Other]);
        assert_eq!(press(&mut controls, &SECRET[5..]), None);
    }

    #[test]
    fn a_held_key_counts_once_in_the_sequence() {
        let mut controls = Controls::default();
        let mut last = None;
        for key in SECRET {
            last = controls.key_down(key, false);
            // The system repeats the key while it is held.
            for _ in 0..5 {
                assert_eq!(controls.key_down(key, true), None);
            }
            controls.key_up(key);
        }
        assert_eq!(last, Some(Command::CallUfo));
        // The repeats of a held key are not a second press: one up and the
        // rest of the sequence is a key short.
        let mut controls = Controls::default();
        controls.key_down(Key::Up, false);
        for _ in 0..5 {
            controls.key_down(Key::Up, true);
        }
        controls.key_up(Key::Up);
        assert_eq!(press(&mut controls, &SECRET[2..]), None);
    }

    #[test]
    fn the_hint_is_shown_for_four_seconds_and_fades_in_half_a_second() {
        let mut hint = Hint::default();
        assert_eq!(hint.opacity(false), 1.0);
        hint.update(3.9);
        assert_eq!(hint.opacity(false), 1.0);
        hint.update(0.35);
        let fading = hint.opacity(false);
        assert!(fading > 0.0 && fading < 1.0, "{fading}");
        hint.update(0.25);
        assert_eq!(hint.opacity(false), 0.0);
        hint.update(100.0);
        assert_eq!(hint.opacity(false), 0.0);
    }

    #[test]
    fn input_brings_the_hint_back_and_a_pause_keeps_it() {
        let mut hint = Hint::default();
        hint.update(10.0);
        assert_eq!(hint.opacity(false), 0.0);
        assert_eq!(hint.opacity(true), 1.0);
        hint.touch();
        assert_eq!(hint.opacity(false), 1.0);
        // Bad steps change nothing.
        for dt in [f32::NAN, -1.0, f32::INFINITY] {
            hint.update(dt);
        }
        assert_eq!(hint.opacity(false), 1.0);
    }
}
