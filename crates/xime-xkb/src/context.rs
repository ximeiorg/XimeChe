use std::fs::File;
use std::os::unix::io::{FromRawFd, OwnedFd, RawFd};
use tracing::debug;
use xkbcommon::xkb::{
    keysym_from_name, Context, Keycode, Keymap, Keysym, ModIndex, State, KEYSYM_CASE_INSENSITIVE,
};

use crate::Error;
use crate::Result;

/// Key binding parsed from string like "Ctrl+Alt+F1"
#[derive(Debug, Clone, Default)]
pub struct KeyBinding {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub keysym: Option<Keysym>,
    pub key_name: String,
}

impl KeyBinding {
    /// Parse a key binding string like "Ctrl+Alt+F1" or "Win+Alt+a"
    pub fn parse(s: &str) -> Self {
        let mut binding = KeyBinding::default();
        let parts: Vec<&str> = s.split('+').collect();

        for part in parts.iter() {
            let trimmed = part.trim();
            match trimmed.to_lowercase().as_str() {
                "ctrl" | "control" => binding.ctrl = true,
                "alt" => binding.alt = true,
                "shift" => binding.shift = true,
                "super" | "win" | "meta" => binding.super_key = true,
                key => {
                    binding.key_name = key.to_uppercase();
                    // Use xkbcommon to parse keysym name (case insensitive)
                    binding.keysym = Some(keysym_from_name(key, KEYSYM_CASE_INSENSITIVE));
                }
            }
        }
        binding
    }

    /// Get the keysym value for Rime
    pub fn keysym_raw(&self) -> Option<u32> {
        self.keysym.map(|k| k.raw())
    }

    /// Check if modifiers match (for hotkey detection) - without super
    pub fn matches_modifiers(&self, ctrl: bool, alt: bool, shift: bool) -> bool {
        self.ctrl == ctrl && self.alt == alt && self.shift == shift
    }

    /// Check if all modifiers match including super/win
    pub fn matches_modifiers_full(
        &self,
        ctrl: bool,
        alt: bool,
        shift: bool,
        super_key: bool,
    ) -> bool {
        self.ctrl == ctrl && self.alt == alt && self.shift == shift && self.super_key == super_key
    }

    /// Get modifier mask for Rime
    pub fn modifier_mask(&self) -> u32 {
        let mut mask = 0u32;
        if self.ctrl {
            mask |= 0x04;
        } // K_CONTROL_MASK
        if self.alt {
            mask |= 0x08;
        } // K_ALT_MASK
        if self.shift {
            mask |= 0x01;
        } // K_SHIFT_MASK
        mask
    }
}

pub struct XkbContext {
    context: Context,
    keymap: Option<Keymap>,
    state: Option<State>,
}

impl XkbContext {
    pub fn new() -> Result<Self> {
        let context = Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);

        Ok(Self {
            context,
            keymap: None,
            state: None,
        })
    }

    pub fn set_keymap_from_fd(&mut self, fd: RawFd, size: usize) -> Result<()> {
        let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
        self.set_keymap_from_owned_fd(owned_fd, size)
    }

    pub fn set_keymap_from_owned_fd(&mut self, owned_fd: OwnedFd, size: usize) -> Result<()> {
        let mut file = File::from(owned_fd);

        debug!("Loading keymap from file (size: {} bytes)", size);

        let keymap = Keymap::new_from_file(
            &self.context,
            &mut file,
            xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1,
            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
        );

        match keymap {
            Some(km) => {
                let state = State::new(&km);
                self.keymap = Some(km);
                self.state = Some(state);
                debug!("Keymap loaded successfully");
                Ok(())
            }
            None => {
                debug!("Keymap creation failed");
                Err(Error::KeymapCreationFailed)
            }
        }
    }

    pub fn key_from_keycode(&self, keycode: u32) -> Option<Keysym> {
        if let Some(state) = &self.state {
            Some(state.key_get_one_sym(Keycode::new(keycode)))
        } else {
            None
        }
    }

    pub fn key_is_modifier(&self, _keycode: u32, modifier: ModIndex) -> Option<bool> {
        if let Some(state) = &self.state {
            Some(state.mod_index_is_active(modifier, xkbcommon::xkb::STATE_MODS_EFFECTIVE))
        } else {
            None
        }
    }

    pub fn get_modifiers(&self) -> ModifierState {
        if let Some(state) = &self.state {
            let depressed = state.serialize_mods(xkbcommon::xkb::STATE_MODS_DEPRESSED);
            let latched = state.serialize_mods(xkbcommon::xkb::STATE_MODS_LATCHED);
            let locked = state.serialize_mods(xkbcommon::xkb::STATE_MODS_LOCKED);
            let effective = state.serialize_mods(xkbcommon::xkb::STATE_MODS_EFFECTIVE);
            let layout = state.serialize_layout(xkbcommon::xkb::STATE_LAYOUT_EFFECTIVE);

            // Check individual modifiers
            let shift = state.mod_index_is_active(0, xkbcommon::xkb::STATE_MODS_EFFECTIVE);
            let ctrl = state.mod_index_is_active(2, xkbcommon::xkb::STATE_MODS_EFFECTIVE);
            let alt = state.mod_index_is_active(1, xkbcommon::xkb::STATE_MODS_EFFECTIVE);
            // Mod4 (index 3) is usually Super/Win
            let super_key = state.mod_index_is_active(3, xkbcommon::xkb::STATE_MODS_EFFECTIVE);

            debug!("XKB: depressed={}, latched={}, locked={}, effective={}, shift={}, ctrl={}, alt={}, super={}", 
                      depressed, latched, locked, effective, shift, ctrl, alt, super_key);

            ModifierState {
                depressed,
                latched,
                locked,
                effective,
                layout,
                shift,
                ctrl,
                alt,
                super_key,
            }
        } else {
            ModifierState::default()
        }
    }

    pub fn update_modifiers(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) {
        if let Some(state) = &mut self.state {
            state.update_mask(depressed, latched, locked, 0, 0, group);
        }
    }
}

impl Default for XkbContext {
    fn default() -> Self {
        Self::new().expect("Failed to create XKB context")
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ModifierState {
    pub depressed: u32,
    pub latched: u32,
    pub locked: u32,
    pub effective: u32,
    pub layout: u32,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
}

pub fn keysym_to_rime_keycode(keysym: Keysym) -> i32 {
    keysym.raw() as i32
}

pub fn keysym_to_char(keysym: Keysym) -> Option<char> {
    let raw = keysym.raw();
    if raw >= 0x61 && raw <= 0x7a {
        Some((raw as u8) as char)
    } else if raw >= 0x41 && raw <= 0x5a {
        Some((raw as u8) as char)
    } else if raw >= 0x30 && raw <= 0x39 {
        Some((raw as u8) as char)
    } else if raw == 0x20 {
        Some(' ')
    } else {
        None
    }
}

/// Convert keysym raw value to lowercase letter (for hotkey matching)
pub fn keysym_to_letter(keysym_raw: u32) -> Option<char> {
    // Lowercase letters: 0x61-0x7a (a-z)
    if keysym_raw >= 0x61 && keysym_raw <= 0x7a {
        Some((keysym_raw as u8 as char).to_ascii_lowercase())
    }
    // Uppercase letters: 0x41-0x5a (A-Z)
    else if keysym_raw >= 0x41 && keysym_raw <= 0x5a {
        Some((keysym_raw as u8 as char).to_ascii_lowercase())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xkbcommon::xkb::{keysym_from_name, KEYSYM_CASE_INSENSITIVE};

    #[test]
    fn test_keybinding_parse_ctrl_only() {
        let binding = KeyBinding::parse("Ctrl");
        assert!(binding.ctrl);
        assert!(!binding.alt);
        assert!(!binding.shift);
        assert!(!binding.super_key);
    }

    #[test]
    fn test_keybinding_parse_control_variant() {
        let binding = KeyBinding::parse("Control");
        assert!(binding.ctrl);
        assert!(!binding.alt);
        assert!(!binding.shift);
        assert!(!binding.super_key);
    }

    #[test]
    fn test_keybinding_parse_alt_only() {
        let binding = KeyBinding::parse("Alt");
        assert!(!binding.ctrl);
        assert!(binding.alt);
        assert!(!binding.shift);
        assert!(!binding.super_key);
    }

    #[test]
    fn test_keybinding_parse_shift_only() {
        let binding = KeyBinding::parse("Shift");
        assert!(!binding.ctrl);
        assert!(!binding.alt);
        assert!(binding.shift);
        assert!(!binding.super_key);
    }

    #[test]
    fn test_keybinding_parse_super_variants() {
        for name in ["Super", "Win", "Meta"] {
            let binding = KeyBinding::parse(name);
            assert!(!binding.ctrl);
            assert!(!binding.alt);
            assert!(!binding.shift);
            assert!(binding.super_key, "Failed for {}", name);
        }
    }

    #[test]
    fn test_keybinding_parse_ctrl_alt_f1() {
        let binding = KeyBinding::parse("Ctrl+Alt+F1");
        assert!(binding.ctrl);
        assert!(binding.alt);
        assert!(!binding.shift);
        assert!(!binding.super_key);
        assert_eq!(binding.key_name, "F1");
        assert!(binding.keysym.is_some());
    }

    #[test]
    fn test_keybinding_parse_win_alt_a() {
        let binding = KeyBinding::parse("Win+Alt+a");
        assert!(!binding.ctrl);
        assert!(binding.alt);
        assert!(!binding.shift);
        assert!(binding.super_key);
        assert_eq!(binding.key_name, "A");
    }

    #[test]
    fn test_keybinding_parse_ctrl_super_space() {
        let binding = KeyBinding::parse("Control+Super+Space");
        assert!(binding.ctrl);
        assert!(!binding.alt);
        assert!(!binding.shift);
        assert!(binding.super_key);
        assert_eq!(binding.key_name, "SPACE");
    }

    #[test]
    fn test_keybinding_parse_single_key() {
        let binding = KeyBinding::parse("a");
        assert!(!binding.ctrl);
        assert!(!binding.alt);
        assert!(!binding.shift);
        assert!(!binding.super_key);
        assert_eq!(binding.key_name, "A");
    }

    #[test]
    fn test_keybinding_parse_with_spaces() {
        let binding = KeyBinding::parse("Ctrl + Alt + F1");
        assert!(binding.ctrl);
        assert!(binding.alt);
        assert!(!binding.shift);
    }

    #[test]
    fn test_keybinding_parse_case_insensitive() {
        let binding = KeyBinding::parse("CTRL+ALT+SHIFT+A");
        assert!(binding.ctrl);
        assert!(binding.alt);
        assert!(binding.shift);
        assert_eq!(binding.key_name, "A");
    }

    #[test]
    fn test_keybinding_matches_modifiers_basic() {
        let binding = KeyBinding {
            ctrl: true,
            alt: false,
            shift: false,
            super_key: false,
            keysym: None,
            key_name: String::new(),
        };
        assert!(binding.matches_modifiers(true, false, false));
        assert!(!binding.matches_modifiers(true, true, false));
        assert!(!binding.matches_modifiers(false, false, false));
    }

    #[test]
    fn test_keybinding_matches_modifiers_multiple() {
        let binding = KeyBinding {
            ctrl: true,
            alt: true,
            shift: false,
            super_key: false,
            keysym: None,
            key_name: String::new(),
        };
        assert!(binding.matches_modifiers(true, true, false));
        assert!(!binding.matches_modifiers(true, false, false));
        assert!(!binding.matches_modifiers(false, true, false));
    }

    #[test]
    fn test_keybinding_matches_modifiers_all() {
        let binding = KeyBinding {
            ctrl: true,
            alt: true,
            shift: true,
            super_key: false,
            keysym: None,
            key_name: String::new(),
        };
        assert!(binding.matches_modifiers(true, true, true));
        assert!(!binding.matches_modifiers(true, true, false));
    }

    #[test]
    fn test_keybinding_matches_modifiers_full_with_super() {
        let binding = KeyBinding {
            ctrl: true,
            alt: true,
            shift: false,
            super_key: true,
            keysym: None,
            key_name: String::new(),
        };
        assert!(binding.matches_modifiers_full(true, true, false, true));
        assert!(!binding.matches_modifiers_full(true, true, false, false));
    }

    #[test]
    fn test_keybinding_modifier_mask_ctrl() {
        let binding = KeyBinding {
            ctrl: true,
            alt: false,
            shift: false,
            super_key: false,
            keysym: None,
            key_name: String::new(),
        };
        assert_eq!(binding.modifier_mask(), 0x04);
    }

    #[test]
    fn test_keybinding_modifier_mask_alt() {
        let binding = KeyBinding {
            ctrl: false,
            alt: true,
            shift: false,
            super_key: false,
            keysym: None,
            key_name: String::new(),
        };
        assert_eq!(binding.modifier_mask(), 0x08);
    }

    #[test]
    fn test_keybinding_modifier_mask_shift() {
        let binding = KeyBinding {
            ctrl: false,
            alt: false,
            shift: true,
            super_key: false,
            keysym: None,
            key_name: String::new(),
        };
        assert_eq!(binding.modifier_mask(), 0x01);
    }

    #[test]
    fn test_keybinding_modifier_mask_combined() {
        let binding = KeyBinding {
            ctrl: true,
            alt: true,
            shift: false,
            super_key: false,
            keysym: None,
            key_name: String::new(),
        };
        assert_eq!(binding.modifier_mask(), 0x0C); // 0x04 | 0x08
    }

    #[test]
    fn test_keybinding_modifier_mask_all() {
        let binding = KeyBinding {
            ctrl: true,
            alt: true,
            shift: true,
            super_key: false,
            keysym: None,
            key_name: String::new(),
        };
        assert_eq!(binding.modifier_mask(), 0x0D); // 0x04 | 0x08 | 0x01
    }

    #[test]
    fn test_keybinding_keysym_raw() {
        let keysym = keysym_from_name("a", KEYSYM_CASE_INSENSITIVE);
        let binding = KeyBinding {
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
            keysym: Some(keysym),
            key_name: "A".to_string(),
        };
        assert!(binding.keysym_raw().is_some());
        assert_eq!(binding.keysym_raw(), Some(0x61));
    }

    #[test]
    fn test_keybinding_keysym_raw_none() {
        let binding = KeyBinding::default();
        assert!(binding.keysym_raw().is_none());
    }

    #[test]
    fn test_keysym_to_char_lowercase_letters() {
        for (c, expected) in [('a', Some('a')), ('z', Some('z')), ('m', Some('m'))] {
            let keysym = keysym_from_name(&c.to_string(), KEYSYM_CASE_INSENSITIVE);
            assert_eq!(keysym_to_char(keysym), expected);
        }
    }

    #[test]
    fn test_keysym_to_char_uppercase_letters() {
        let keysym_a_upper = keysym_from_name("A", KEYSYM_CASE_INSENSITIVE);
        let keysym_z_upper = keysym_from_name("Z", KEYSYM_CASE_INSENSITIVE);
        let keysym_m_upper = keysym_from_name("M", KEYSYM_CASE_INSENSITIVE);
        assert_eq!(keysym_to_char(keysym_a_upper), Some('a'));
        assert_eq!(keysym_to_char(keysym_z_upper), Some('z'));
        assert_eq!(keysym_to_char(keysym_m_upper), Some('m'));
    }

    #[test]
    fn test_keysym_to_char_digits() {
        for d in '0'..='9' {
            let keysym = keysym_from_name(&d.to_string(), KEYSYM_CASE_INSENSITIVE);
            assert_eq!(keysym_to_char(keysym), Some(d));
        }
    }

    #[test]
    fn test_keysym_to_char_space() {
        let keysym = keysym_from_name("space", KEYSYM_CASE_INSENSITIVE);
        assert_eq!(keysym_to_char(keysym), Some(' '));
    }

    #[test]
    fn test_keysym_to_char_non_printable() {
        let keysym = keysym_from_name("F1", KEYSYM_CASE_INSENSITIVE);
        assert_eq!(keysym_to_char(keysym), None);
    }

    #[test]
    fn test_keysym_to_letter_lowercase() {
        for c in 'a'..='z' {
            let keysym_raw = c as u32;
            assert_eq!(keysym_to_letter(keysym_raw), Some(c));
        }
    }

    #[test]
    fn test_keysym_to_letter_uppercase_to_lowercase() {
        for c in 'A'..='Z' {
            let keysym_raw = c as u32;
            let expected = c.to_ascii_lowercase();
            assert_eq!(keysym_to_letter(keysym_raw), Some(expected));
        }
    }

    #[test]
    fn test_keysym_to_letter_digits_returns_none() {
        for d in '0'..='9' {
            let keysym_raw = d as u32;
            assert_eq!(keysym_to_letter(keysym_raw), None);
        }
    }

    #[test]
    fn test_keysym_to_letter_special_returns_none() {
        assert_eq!(keysym_to_letter(0x20), None); // space
        assert_eq!(keysym_to_letter(0xFF), None); // non-printable
    }

    #[test]
    fn test_keysym_to_rime_keycode() {
        let keysym = keysym_from_name("a", KEYSYM_CASE_INSENSITIVE);
        assert_eq!(keysym_to_rime_keycode(keysym), 0x61);
    }

    #[test]
    fn test_modifier_state_default() {
        let state = ModifierState::default();
        assert_eq!(state.depressed, 0);
        assert_eq!(state.latched, 0);
        assert_eq!(state.locked, 0);
        assert_eq!(state.effective, 0);
        assert!(!state.shift);
        assert!(!state.ctrl);
        assert!(!state.alt);
        assert!(!state.super_key);
    }

    #[test]
    fn test_keybinding_default() {
        let binding = KeyBinding::default();
        assert!(!binding.ctrl);
        assert!(!binding.alt);
        assert!(!binding.shift);
        assert!(!binding.super_key);
        assert!(binding.keysym.is_none());
        assert_eq!(binding.key_name, "");
    }
}
