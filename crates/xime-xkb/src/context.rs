use xkbcommon::xkb::{Context, Keymap, State, Keysym, Keycode, ModIndex};
use std::os::unix::io::{RawFd, FromRawFd, OwnedFd};
use std::io::Read;
use std::fs::File;

use crate::Error;
use crate::Result;

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
            
            eprintln!("DEBUG XKB: depressed={}, latched={}, locked={}, effective={}", depressed, latched, locked, effective);
            
            ModifierState {
                depressed,
                latched,
                locked,
                effective,
                layout,
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