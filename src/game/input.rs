//! Key bindings: which key drives each player action.
//!
//! The original lets both players' keys be reassigned (a native dialog on
//! Windows, InputSprocket on the Mac) and keeps them in its preferences.
//! The remake keeps them in a small text file beside its other runtime
//! files, edited from the Controls screen.

use bevy::prelude::*;

/// One player's controls for a tick.
#[derive(Clone, Copy, Default)]
pub struct SlotInput {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub fire: bool,
    pub bomb: bool,
    /// Select/cycle air weapon (original input[6]).
    pub cycle: bool,
}

impl SlotInput {
    /// The port's input order: up, right, down, left, ground weapon, air
    /// weapon, cycle.
    pub fn og(self) -> [bool; 7] {
        [self.up, self.right, self.down, self.left, self.bomb, self.fire, self.cycle]
    }
}

/// Player actions in binding order: `(config key, on-screen label)`.
pub const ACTIONS: [(&str, &str); 7] = [
    ("up", "Move Up"),
    ("down", "Move Down"),
    ("left", "Move Left"),
    ("right", "Move Right"),
    ("fire", "Fire Air Weapon"),
    ("bomb", "Fire Ground Weapon"),
    ("cycle", "Select Air Weapon"),
];

pub const UP: usize = 0;
pub const DOWN: usize = 1;
pub const LEFT: usize = 2;
pub const RIGHT: usize = 3;
pub const FIRE: usize = 4;
pub const BOMB: usize = 5;
pub const CYCLE: usize = 6;

const KEYS_PATH: &str = "deimos-keys.txt";

/// The key bound to each action, per player.
#[derive(Resource, Clone, PartialEq)]
pub struct KeyBindings {
    pub keys: [[KeyCode; 7]; 2],
}

impl Default for KeyBindings {
    fn default() -> Self {
        KeyBindings {
            keys: [
                // The Windows exe's defaults (U_Prefs, tables written at VA
                // 0x408f12 and 0x408f47, read by G_Input_CachePlayerInputs).
                // Player 1: arrows, LCtrl / LAlt / LShift for air weapon,
                // ground weapon, weapon select.
                [
                    KeyCode::ArrowUp,
                    KeyCode::ArrowDown,
                    KeyCode::ArrowLeft,
                    KeyCode::ArrowRight,
                    KeyCode::ControlLeft,
                    KeyCode::AltLeft,
                    KeyCode::ShiftLeft,
                ],
                // Player 2: KPad 8/5/4/6, End, Delete, PgDn.
                [
                    KeyCode::Numpad8,
                    KeyCode::Numpad5,
                    KeyCode::Numpad4,
                    KeyCode::Numpad6,
                    KeyCode::End,
                    KeyCode::Delete,
                    KeyCode::PageDown,
                ],
            ],
        }
    }
}

impl KeyBindings {
    /// Load saved bindings, falling back to the defaults for anything
    /// missing or unreadable.
    pub fn load() -> Self {
        let mut b = KeyBindings::default();
        if let Ok(text) = std::fs::read_to_string(KEYS_PATH) {
            b.apply(&text);
        }
        b
    }

    /// Apply `p<N>.<action> = <KeyName>` lines; unknown lines are ignored.
    pub fn apply(&mut self, text: &str) {
        for line in text.lines() {
            let Some((lhs, rhs)) = line.split_once('=') else { continue };
            let Some((player, action)) = lhs.trim().split_once('.') else { continue };
            let slot = match player {
                "p1" => 0,
                "p2" => 1,
                _ => continue,
            };
            let Some(index) = ACTIONS.iter().position(|a| a.0 == action) else { continue };
            if let Some(key) = key_from_name(rhs.trim()) {
                self.keys[slot][index] = key;
            }
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (slot, keys) in self.keys.iter().enumerate() {
            for (index, key) in keys.iter().enumerate() {
                out.push_str(&format!("p{}.{} = {:?}\n", slot + 1, ACTIONS[index].0, key));
            }
        }
        out
    }

    pub fn save(&self) {
        let _ = std::fs::write(KEYS_PATH, self.to_text());
    }

    /// Bind `key` to an action. Like the original's dialog (sub_44a450),
    /// a key another action already uses is refused: returns false and
    /// changes nothing.
    pub fn bind(&mut self, slot: usize, index: usize, key: KeyCode) -> bool {
        for s in 0..2 {
            for i in 0..7 {
                if (s, i) != (slot, index) && self.keys[s][i] == key {
                    return false;
                }
            }
        }
        self.keys[slot][index] = key;
        true
    }
}

const PAD_PATH: &str = "deimos-pad.txt";

/// The pad button bound to each action, per player. The left stick moves
/// as well whatever the movement buttons are, as the original's joystick
/// axes always did.
#[derive(Resource, Clone, PartialEq)]
pub struct PadBindings {
    pub buttons: [[GamepadButton; 7]; 2],
}

impl Default for PadBindings {
    fn default() -> Self {
        // The original's joystick defaults: the POV hat moves, buttons 1, 2
        // and 3 fire the air weapon, fire the ground weapon and select the
        // air weapon.
        let pad = [
            GamepadButton::DPadUp,
            GamepadButton::DPadDown,
            GamepadButton::DPadLeft,
            GamepadButton::DPadRight,
            GamepadButton::South,
            GamepadButton::East,
            GamepadButton::West,
        ];
        PadBindings { buttons: [pad; 2] }
    }
}

impl PadBindings {
    /// Load saved bindings, falling back to the defaults for anything
    /// missing or unreadable.
    pub fn load() -> Self {
        let mut b = PadBindings::default();
        if let Ok(text) = std::fs::read_to_string(PAD_PATH) {
            b.apply(&text);
        }
        b
    }

    /// Apply `p<N>.<action> = <ButtonName>` lines; unknown lines are ignored.
    pub fn apply(&mut self, text: &str) {
        for line in text.lines() {
            let Some((lhs, rhs)) = line.split_once('=') else { continue };
            let Some((player, action)) = lhs.trim().split_once('.') else { continue };
            let slot = match player {
                "p1" => 0,
                "p2" => 1,
                _ => continue,
            };
            let Some(index) = ACTIONS.iter().position(|a| a.0 == action) else { continue };
            if let Some(button) = button_from_name(rhs.trim()) {
                self.buttons[slot][index] = button;
            }
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (slot, buttons) in self.buttons.iter().enumerate() {
            for (index, button) in buttons.iter().enumerate() {
                out.push_str(&format!("p{}.{} = {:?}\n", slot + 1, ACTIONS[index].0, button));
            }
        }
        out
    }

    pub fn save(&self) {
        let _ = std::fs::write(PAD_PATH, self.to_text());
    }

    /// Bind `button` to one of a player's actions. Each player has a pad
    /// of their own, so only that player's other actions can clash; like
    /// the keys, a button already in use is refused.
    pub fn bind(&mut self, slot: usize, index: usize, button: GamepadButton) -> bool {
        if (0..7).any(|i| i != index && self.buttons[slot][i] == button) {
            return false;
        }
        self.buttons[slot][index] = button;
        true
    }
}

/// Pad buttons that may be bound. Start and the guide button are left
/// out: Start drives the menus.
const BINDABLE_BUTTONS: &[GamepadButton] = &[
    GamepadButton::South,
    GamepadButton::East,
    GamepadButton::North,
    GamepadButton::West,
    GamepadButton::C,
    GamepadButton::Z,
    GamepadButton::LeftTrigger,
    GamepadButton::RightTrigger,
    GamepadButton::LeftTrigger2,
    GamepadButton::RightTrigger2,
    GamepadButton::LeftThumb,
    GamepadButton::RightThumb,
    GamepadButton::Select,
    GamepadButton::DPadUp,
    GamepadButton::DPadDown,
    GamepadButton::DPadLeft,
    GamepadButton::DPadRight,
];

/// Parse a button by the name `PadBindings::to_text` writes.
pub fn button_from_name(name: &str) -> Option<GamepadButton> {
    BINDABLE_BUTTONS.iter().copied().find(|b| format!("{:?}", b) == name)
}

/// Name of a pad button on the Controls screen, by its place on an Xbox
/// style pad.
pub fn button_label(button: GamepadButton) -> &'static str {
    use GamepadButton::*;
    match button {
        South => "A",
        East => "B",
        North => "Y",
        West => "X",
        C => "C",
        Z => "Z",
        LeftTrigger => "LB",
        RightTrigger => "RB",
        LeftTrigger2 => "LT",
        RightTrigger2 => "RT",
        LeftThumb => "L3",
        RightThumb => "R3",
        Select => "Back",
        Start => "Start",
        Mode => "Guide",
        DPadUp => "D-Up",
        DPadDown => "D-Down",
        DPadLeft => "D-Left",
        DPadRight => "D-Right",
        _ => "?",
    }
}

/// Game controllers, read the way the original reads joysticks
/// (G_Input_CachePlayerInputs with the U_Prefs defaults): the first
/// connected pad plays player 1, the second player 2. The left stick
/// moves once past 20% of its travel (`JoystickSetDigital(.., 20)`); the
/// buttons are those of `PadBindings`.
#[derive(Resource, Default)]
pub struct Pads {
    /// Per player, per action in `ACTIONS` order: held this frame.
    pub now: [[bool; 7]; 2],
    /// The same for the previous frame.
    pub prev: [[bool; 7]; 2],
    /// Menu buttons newly pressed this frame, any pad.
    pub menu: MenuPad,
    /// Stick directions held last frame, any pad, in `MenuPad` order.
    stick_prev: [bool; 4],
    /// The first bindable button newly pressed this frame, any pad: what
    /// the Controls screen binds.
    pub new_button: Option<GamepadButton>,
    /// Per player, the share of the ship's top speed along x and y, from
    /// how far the left stick is pushed (`stick_share`); 1.0 when the
    /// stick is centred on the axis or a button moves along it.
    pub move_scale: [[f32; 2]; 2],
}

/// A pad's menu buttons, newly pressed: the D-pad or left stick for the
/// arrows, A or Start for Enter, B for Esc. Fixed, whatever the game's
/// pad bindings, so the menus always work the same.
#[derive(Clone, Copy, Default)]
pub struct MenuPad {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub ok: bool,
    pub back: bool,
    /// Start alone, which also counts as `ok`.
    pub start: bool,
}

/// Stick deflection the original counts as a press.
const STICK_THRESHOLD: f32 = 0.2;

impl Pads {
    pub fn pressed(&self, slot: usize, action: usize) -> bool {
        self.now[slot][action]
    }

    pub fn just_pressed(&self, slot: usize, action: usize) -> bool {
        self.now[slot][action] && !self.prev[slot][action]
    }

}

/// Stick deflection that already counts as full speed: pads rarely
/// reach 1.0, least of all on a diagonal.
const STICK_FULL: f32 = 0.9;

/// The share of the top speed for a stick pushed `amount` (0..=1) along
/// an axis: proportional, reaching 1.0 at `STICK_FULL`.
pub fn stick_share(amount: f32) -> f32 {
    (amount.abs() / STICK_FULL).min(1.0)
}

/// Left stick directions past the threshold: up, down, left, right.
fn stick_dirs(pad: &Gamepad) -> [bool; 4] {
    let x = pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
    let y = pad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
    [y > STICK_THRESHOLD, y < -STICK_THRESHOLD, x < -STICK_THRESHOLD, x > STICK_THRESHOLD]
}

/// Samples the connected pads once a frame.
pub fn read_pads(mut pads: ResMut<Pads>, bindings: Res<PadBindings>, q: Query<(Entity, &Gamepad)>) {
    pads.prev = pads.now;
    pads.now = [[false; 7]; 2];
    pads.move_scale = [[1.0; 2]; 2];
    let mut menu = MenuPad::default();
    let mut stick = [false; 4];
    let mut new_button = None;
    let mut list: Vec<(Entity, &Gamepad)> = q.iter().collect();
    list.sort_by_key(|(e, _)| *e);
    for (slot, (_, pad)) in list.iter().enumerate() {
        let dirs = stick_dirs(pad);
        for d in 0..4 {
            stick[d] |= dirs[d];
        }
        menu.up |= pad.just_pressed(GamepadButton::DPadUp);
        menu.down |= pad.just_pressed(GamepadButton::DPadDown);
        menu.left |= pad.just_pressed(GamepadButton::DPadLeft);
        menu.right |= pad.just_pressed(GamepadButton::DPadRight);
        menu.ok |= pad.just_pressed(GamepadButton::South) || pad.just_pressed(GamepadButton::Start);
        menu.back |= pad.just_pressed(GamepadButton::East);
        menu.start |= pad.just_pressed(GamepadButton::Start);
        new_button = new_button
            .or_else(|| BINDABLE_BUTTONS.iter().copied().find(|b| pad.just_pressed(*b)));
        if slot >= 2 {
            continue;
        }
        let mut a: [bool; 7] = std::array::from_fn(|i| pad.pressed(bindings.buttons[slot][i]));
        // A stick alone on an axis steers at its share of the top speed.
        let x = pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        let y = pad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
        if !a[LEFT] && !a[RIGHT] && (dirs[2] || dirs[3]) {
            pads.move_scale[slot][0] = stick_share(x);
        }
        if !a[UP] && !a[DOWN] && (dirs[0] || dirs[1]) {
            pads.move_scale[slot][1] = stick_share(y);
        }
        a[UP] |= dirs[0];
        a[DOWN] |= dirs[1];
        a[LEFT] |= dirs[2];
        a[RIGHT] |= dirs[3];
        pads.now[slot] = a;
    }
    let newly = |d: usize| stick[d] && !pads.stick_prev[d];
    menu.up |= newly(0);
    menu.down |= newly(1);
    menu.left |= newly(2);
    menu.right |= newly(3);
    pads.stick_prev = stick;
    pads.menu = menu;
    pads.new_button = new_button;
}

/// Keys that may be bound. Escape, Enter and the function keys are left
/// out: the menus and the game's own shortcuts use them.
const BINDABLE: &[KeyCode] = &[
    KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE,
    KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ,
    KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN, KeyCode::KeyO,
    KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT,
    KeyCode::KeyU, KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY,
    KeyCode::KeyZ,
    KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4,
    KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
    KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight,
    KeyCode::Space, KeyCode::Tab, KeyCode::Backspace,
    KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::ControlLeft, KeyCode::ControlRight,
    KeyCode::AltLeft, KeyCode::AltRight, KeyCode::SuperLeft, KeyCode::SuperRight,
    KeyCode::Comma, KeyCode::Period, KeyCode::Slash, KeyCode::Semicolon, KeyCode::Quote,
    KeyCode::BracketLeft, KeyCode::BracketRight, KeyCode::Backslash, KeyCode::Minus,
    KeyCode::Equal, KeyCode::Backquote,
    KeyCode::Numpad0, KeyCode::Numpad1, KeyCode::Numpad2, KeyCode::Numpad3, KeyCode::Numpad4,
    KeyCode::Numpad5, KeyCode::Numpad6, KeyCode::Numpad7, KeyCode::Numpad8, KeyCode::Numpad9,
    KeyCode::NumpadAdd, KeyCode::NumpadSubtract, KeyCode::NumpadMultiply,
    KeyCode::NumpadDivide, KeyCode::NumpadDecimal, KeyCode::NumpadEnter,
    KeyCode::Home, KeyCode::End, KeyCode::PageUp, KeyCode::PageDown, KeyCode::Insert,
    KeyCode::Delete,
];

pub fn is_bindable(key: KeyCode) -> bool {
    BINDABLE.contains(&key)
}

/// Parse a key by the name `to_text` writes (its `Debug` form).
pub fn key_from_name(name: &str) -> Option<KeyCode> {
    BINDABLE.iter().copied().find(|k| format!("{:?}", k) == name)
}

/// Name of a key on the Controls screen: the Windows exe's key name
/// table (VA 0x503830, by scan code). It names scan code 0x1a, the `[`
/// key, "]" and 0x1b "[", and so does this. The Mac's Command keys have
/// no Windows scan code.
pub fn key_label(key: KeyCode) -> String {
    use KeyCode::*;
    let name = match key {
        Escape => "Escape",
        Minus => "-",
        Equal => "=",
        Backspace => "BakSpc",
        Tab => "Tab",
        BracketLeft => "]",
        BracketRight => "[",
        Semicolon => ";",
        Quote => "'",
        Backquote => "`",
        Backslash => "\\",
        Comma => ",",
        Period => ".",
        Slash => "/",
        Enter => "Enter",
        ControlLeft => "LCtrl",
        ControlRight => "RCtrl",
        ShiftLeft => "LShift",
        ShiftRight => "RShift",
        AltLeft => "LAlt",
        AltRight => "RAlt",
        SuperLeft => "LCmd",
        SuperRight => "RCmd",
        Space => "Space",
        CapsLock => "CapLck",
        NumLock => "NumLck",
        ScrollLock => "ScrLck",
        Pause => "Pause",
        ArrowUp => "Up",
        ArrowDown => "Down",
        ArrowLeft => "Left",
        ArrowRight => "Right",
        Insert => "Insert",
        Delete => "Delete",
        Home => "Home",
        End => "End",
        PageUp => "PgUp",
        PageDown => "PgDn",
        PrintScreen => "PrtScn",
        NumpadMultiply => "KPad*",
        NumpadSubtract => "KPad-",
        NumpadAdd => "KPad+",
        NumpadDecimal => "KPad.",
        NumpadDivide => "KPad/",
        NumpadEnter => "KPdEnt",
        _ => "",
    };
    if !name.is_empty() {
        return name.to_string();
    }
    let debug = format!("{:?}", key);
    for prefix in ["Key", "Digit"] {
        if let Some(rest) = debug.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    if let Some(rest) = debug.strip_prefix("Numpad") {
        return format!("KPad{rest}");
    }
    debug
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_round_trip_through_text() {
        let mut b = KeyBindings::default();
        b.bind(0, FIRE, KeyCode::Space);
        b.bind(1, UP, KeyCode::KeyW);
        let mut loaded = KeyBindings::default();
        loaded.apply(&b.to_text());
        assert!(loaded == b);
    }

    #[test]
    fn binding_a_used_key_is_refused() {
        let mut b = KeyBindings::default();
        // LAlt is player 1's ground weapon; End is player 2's air weapon.
        assert!(!b.bind(0, FIRE, KeyCode::AltLeft));
        assert!(!b.bind(0, FIRE, KeyCode::End));
        assert_eq!(b.keys[0][FIRE], KeyCode::ControlLeft);
        // Rebinding an action to its own key is fine.
        assert!(b.bind(0, FIRE, KeyCode::ControlLeft));
    }

    #[test]
    fn labels_use_the_original_names() {
        assert_eq!(key_label(KeyCode::ControlLeft), "LCtrl");
        assert_eq!(key_label(KeyCode::Numpad8), "KPad8");
        assert_eq!(key_label(KeyCode::PageDown), "PgDn");
        assert_eq!(key_label(KeyCode::KeyQ), "Q");
    }

    #[test]
    fn pad_bindings_round_trip_through_text() {
        let mut b = PadBindings::default();
        assert!(b.bind(0, FIRE, GamepadButton::RightTrigger2));
        assert!(b.bind(1, CYCLE, GamepadButton::North));
        let mut loaded = PadBindings::default();
        loaded.apply(&b.to_text());
        assert!(loaded == b);
    }

    #[test]
    fn pad_buttons_clash_only_within_a_player() {
        let mut b = PadBindings::default();
        // B is player 1's ground weapon.
        assert!(!b.bind(0, FIRE, GamepadButton::East));
        assert_eq!(b.buttons[0][FIRE], GamepadButton::South);
        // Each player has a pad of their own: both may use the same button.
        assert!(b.bind(1, CYCLE, GamepadButton::RightTrigger));
        assert!(b.bind(0, CYCLE, GamepadButton::RightTrigger));
        // Start drives the menus and is never read back.
        let mut loaded = PadBindings::default();
        loaded.apply("p1.fire = Start\n");
        assert!(loaded == PadBindings::default());
    }

    #[test]
    fn stick_share_is_proportional_up_to_full() {
        assert_eq!(stick_share(0.45), 0.5);
        assert_eq!(stick_share(-0.45), 0.5);
        assert_eq!(stick_share(STICK_FULL), 1.0);
        assert_eq!(stick_share(1.0), 1.0);
    }

    #[test]
    fn unknown_lines_leave_defaults() {
        let mut b = KeyBindings::default();
        b.apply("p1.fire = NotAKey\np3.up = KeyQ\nnonsense\n");
        assert!(b == KeyBindings::default());
    }
}
