//! Key bindings: which key drives each player action.
//!
//! The original lets both players' keys be reassigned (a native dialog on
//! Windows, InputSprocket on the Mac) and keeps them in its preferences.
//! The remake keeps them in a small text file beside its other runtime
//! files, edited from the Controls screen.

use bevy::prelude::*;

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
                // Player 1: arrows to move, Z / X / C for air weapon,
                // ground weapon, weapon select.
                [
                    KeyCode::ArrowUp,
                    KeyCode::ArrowDown,
                    KeyCode::ArrowLeft,
                    KeyCode::ArrowRight,
                    KeyCode::KeyZ,
                    KeyCode::KeyX,
                    KeyCode::KeyC,
                ],
                // Player 2: the Windows exe's defaults (table written at
                // VA 0x408f47): Numpad 8/5/4/6, End, Delete, PageDown.
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

    /// Bind `key` to an action. A key already used elsewhere swaps places
    /// with the one it replaces, so no action is left without a key.
    pub fn bind(&mut self, slot: usize, index: usize, key: KeyCode) {
        let old = self.keys[slot][index];
        for s in 0..2 {
            for i in 0..7 {
                if self.keys[s][i] == key {
                    self.keys[s][i] = old;
                }
            }
        }
        self.keys[slot][index] = key;
    }
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

/// Short name for a key, as shown on the Controls screen.
pub fn key_label(key: KeyCode) -> String {
    let name = format!("{:?}", key);
    let fixed = match key {
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::ShiftLeft => "Left Shift",
        KeyCode::ShiftRight => "Right Shift",
        KeyCode::ControlLeft => "Left Ctrl",
        KeyCode::ControlRight => "Right Ctrl",
        KeyCode::AltLeft => "Left Alt",
        KeyCode::AltRight => "Right Alt",
        KeyCode::SuperLeft => "Left Cmd",
        KeyCode::SuperRight => "Right Cmd",
        KeyCode::PageUp => "Page Up",
        KeyCode::PageDown => "Page Down",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::BracketLeft => "(",
        KeyCode::BracketRight => ")",
        KeyCode::Backslash => "\\",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::Backquote => "`",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    if let Some(letter) = name.strip_prefix("Key") {
        return letter.to_string();
    }
    if let Some(digit) = name.strip_prefix("Digit") {
        return digit.to_string();
    }
    if let Some(rest) = name.strip_prefix("Numpad") {
        return format!("Num {}", rest);
    }
    name
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
    fn binding_a_used_key_swaps_it() {
        let mut b = KeyBindings::default();
        // X is player 1's ground weapon; give it to the air weapon.
        b.bind(0, FIRE, KeyCode::KeyX);
        assert_eq!(b.keys[0][FIRE], KeyCode::KeyX);
        assert_eq!(b.keys[0][BOMB], KeyCode::KeyZ);
    }

    #[test]
    fn unknown_lines_leave_defaults() {
        let mut b = KeyBindings::default();
        b.apply("p1.fire = NotAKey\np3.up = KeyQ\nnonsense\n");
        assert!(b == KeyBindings::default());
    }
}
