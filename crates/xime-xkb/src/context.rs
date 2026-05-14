use xkbcommon::xkb::{Context, Keymap, State, Keysym, Keycode, ModIndex, keysym_from_name, KEYSYM_CASE_INSENSITIVE};
use std::os::unix::io::{RawFd, FromRawFd, OwnedFd};
use std::fs::File;

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
    pub fn matches_modifiers_full(&self, ctrl: bool, alt: bool, shift: bool, super_key: bool) -> bool {
        self.ctrl == ctrl && self.alt == alt && self.shift == shift && self.super_key == super_key
    }
    
    /// Get modifier mask for Rime
    pub fn modifier_mask(&self) -> u32 {
        let mut mask = 0u32;
        if self.ctrl { mask |= 0x04; }  // K_CONTROL_MASK
        if self.alt { mask |= 0x08; }   // K_ALT_MASK
        if self.shift { mask |= 0x01; } // K_SHIFT_MASK
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
        // 使用 new_from_file 避免 CString::new 的 NUL 字符 panic
        let mut file = File::from(owned_fd);
        
        eprintln!("DEBUG: Loading keymap from file (size: {} bytes)", size);
        
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
                eprintln!("DEBUG: Keymap loaded successfully");
                Ok(())
            }
            None => {
                eprintln!("DEBUG: Keymap creation failed");
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
            
            eprintln!("DEBUG XKB: depressed={}, latched={}, locked={}, effective={}, shift={}, ctrl={}, alt={}, super={}", 
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
    }
    else {
        None
    }
}