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
        let file = File::from(owned_fd);
        let mut buffer = Vec::with_capacity(size);
        file.take(size as u64).read_to_end(&mut buffer)
            .map_err(|_| Error::KeymapCreationFailed)?;
        
        let keymap_str = String::from_utf8(buffer)
            .map_err(|_| Error::InvalidKeymapFormat)?;
        
        let keymap = Keymap::new_from_string(
            &self.context,
            keymap_str,
            xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1,
            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
        ).ok_or(Error::KeymapCreationFailed)?;

        let state = State::new(&keymap);
        
        self.keymap = Some(keymap);
        self.state = Some(state);
        
        Ok(())
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
            ModifierState {
                depressed: state.serialize_mods(xkbcommon::xkb::STATE_MODS_DEPRESSED),
                latched: state.serialize_mods(xkbcommon::xkb::STATE_MODS_LATCHED),
                locked: state.serialize_mods(xkbcommon::xkb::STATE_MODS_LOCKED),
                effective: state.serialize_mods(xkbcommon::xkb::STATE_MODS_EFFECTIVE),
                layout: state.serialize_layout(xkbcommon::xkb::STATE_LAYOUT_EFFECTIVE),
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