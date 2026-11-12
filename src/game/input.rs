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

/// Game controllers, read the way the original reads joysticks
/// (G_Input_CachePlayerInputs with the U_Prefs defaults): the first
/// connected pad plays player 1, the second player 2. The left stick
/// counts past 20% of its travel (`JoystickSetDigital(.., 20)`) and the
/// D-pad (the original's POV hat) moves too; button 1 (A) fires the air
/// weapon, button 2 (B) the ground weapon and button 3 (X) selects the
/// air weapon.
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

/// Left stick directions past the threshold: up, down, left, right.
fn stick_dirs(pad: &Gamepad) -> [bool; 4] {
    let x = pad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
    let y = pad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
    [y > STICK_THRESHOLD, y < -STICK_THRESHOLD, x < -STICK_THRESHOLD, x > STICK_THRESHOLD]
}

/// Samples the connected pads once a frame.
pub fn read_pads(mut pads: ResMut<Pads>, q: Query<(Entity, &Gamepad)>) {
    pads.prev = pads.now;
    pads.now = [[false; 7]; 2];
    let mut menu = MenuPad::default();
    let mut stick = [false; 4];
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
        if slot >= 2 {
            continue;
        }
        let mut a = [false; 7];
        a[UP] = dirs[0] || pad.pressed(GamepadButton::DPadUp);
        a[DOWN] = dirs[1] || pad.pressed(GamepadButton::DPadDown);
        a[LEFT] = dirs[2] || pad.pressed(GamepadButton::DPadLeft);
        a[RIGHT] = dirs[3] || pad.pressed(GamepadButton::DPadRight);
        a[FIRE] = pad.pressed(GamepadButton::South);
        a[BOMB] = pad.pressed(GamepadButton::East);
        a[CYCLE] = pad.pressed(GamepadButton::West);
        pads.now[slot] = a;
    }
    let newly = |d: usize| stick[d] && !pads.stick_prev[d];
    menu.up |= newly(0);
    menu.down |= newly(1);
    menu.left |= newly(2);
    menu.right |= newly(3);
    pads.stick_prev = stick;
    pads.menu = menu;
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
    fn unknown_lines_leave_defaults() {
        let mut b = KeyBindings::default();
        b.apply("p1.fire = NotAKey\np3.up = KeyQ\nnonsense\n");
        assert!(b == KeyBindings::default());
    }
}
