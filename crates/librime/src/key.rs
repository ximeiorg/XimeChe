pub type KeyCode = u32;
pub type Modifier = u32;

pub const XK_BackSpace: KeyCode = librime_sys::RimeKeyCode_XK_BackSpace;
pub const XK_Tab: KeyCode = librime_sys::RimeKeyCode_XK_Tab;
pub const XK_Return: KeyCode = librime_sys::RimeKeyCode_XK_Return;
pub const XK_Escape: KeyCode = librime_sys::RimeKeyCode_XK_Escape;
pub const XK_Delete: KeyCode = librime_sys::RimeKeyCode_XK_Delete;
pub const XK_space: KeyCode = librime_sys::RimeKeyCode_XK_space;
pub const XK_Left: KeyCode = librime_sys::RimeKeyCode_XK_Left;
pub const XK_Up: KeyCode = librime_sys::RimeKeyCode_XK_Up;
pub const XK_Right: KeyCode = librime_sys::RimeKeyCode_XK_Right;
pub const XK_Down: KeyCode = librime_sys::RimeKeyCode_XK_Down;
pub const XK_Prior: KeyCode = librime_sys::RimeKeyCode_XK_Prior;
pub const XK_Next: KeyCode = librime_sys::RimeKeyCode_XK_Next;
pub const XK_Home: KeyCode = librime_sys::RimeKeyCode_XK_Home;
pub const XK_End: KeyCode = librime_sys::RimeKeyCode_XK_End;

pub const XK_a: KeyCode = librime_sys::RimeKeyCode_XK_a;
pub const XK_b: KeyCode = librime_sys::RimeKeyCode_XK_b;
pub const XK_c: KeyCode = librime_sys::RimeKeyCode_XK_c;
pub const XK_d: KeyCode = librime_sys::RimeKeyCode_XK_d;
pub const XK_e: KeyCode = librime_sys::RimeKeyCode_XK_e;
pub const XK_f: KeyCode = librime_sys::RimeKeyCode_XK_f;
pub const XK_g: KeyCode = librime_sys::RimeKeyCode_XK_g;
pub const XK_h: KeyCode = librime_sys::RimeKeyCode_XK_h;
pub const XK_i: KeyCode = librime_sys::RimeKeyCode_XK_i;
pub const XK_j: KeyCode = librime_sys::RimeKeyCode_XK_j;
pub const XK_k: KeyCode = librime_sys::RimeKeyCode_XK_k;
pub const XK_l: KeyCode = librime_sys::RimeKeyCode_XK_l;
pub const XK_m: KeyCode = librime_sys::RimeKeyCode_XK_m;
pub const XK_n: KeyCode = librime_sys::RimeKeyCode_XK_n;
pub const XK_o: KeyCode = librime_sys::RimeKeyCode_XK_o;
pub const XK_p: KeyCode = librime_sys::RimeKeyCode_XK_p;
pub const XK_q: KeyCode = librime_sys::RimeKeyCode_XK_q;
pub const XK_r: KeyCode = librime_sys::RimeKeyCode_XK_r;
pub const XK_s: KeyCode = librime_sys::RimeKeyCode_XK_s;
pub const XK_t: KeyCode = librime_sys::RimeKeyCode_XK_t;
pub const XK_u: KeyCode = librime_sys::RimeKeyCode_XK_u;
pub const XK_v: KeyCode = librime_sys::RimeKeyCode_XK_v;
pub const XK_w: KeyCode = librime_sys::RimeKeyCode_XK_w;
pub const XK_x: KeyCode = librime_sys::RimeKeyCode_XK_x;
pub const XK_y: KeyCode = librime_sys::RimeKeyCode_XK_y;
pub const XK_z: KeyCode = librime_sys::RimeKeyCode_XK_z;

pub const XK_0: KeyCode = librime_sys::RimeKeyCode_XK_0;
pub const XK_1: KeyCode = librime_sys::RimeKeyCode_XK_1;
pub const XK_2: KeyCode = librime_sys::RimeKeyCode_XK_2;
pub const XK_3: KeyCode = librime_sys::RimeKeyCode_XK_3;
pub const XK_4: KeyCode = librime_sys::RimeKeyCode_XK_4;
pub const XK_5: KeyCode = librime_sys::RimeKeyCode_XK_5;
pub const XK_6: KeyCode = librime_sys::RimeKeyCode_XK_6;
pub const XK_7: KeyCode = librime_sys::RimeKeyCode_XK_7;
pub const XK_8: KeyCode = librime_sys::RimeKeyCode_XK_8;
pub const XK_9: KeyCode = librime_sys::RimeKeyCode_XK_9;

pub const K_SHIFT_MASK: Modifier = librime_sys::RimeModifier_kShiftMask;
pub const K_CONTROL_MASK: Modifier = librime_sys::RimeModifier_kControlMask;
pub const K_ALT_MASK: Modifier = librime_sys::RimeModifier_kAltMask;
pub const K_RELEASE_MASK: Modifier = librime_sys::RimeModifier_kReleaseMask;

#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    pub key_code: KeyCode,
    pub modifiers: Modifier,
}

impl KeyEvent {
    pub fn new(key_code: KeyCode, modifiers: Modifier) -> Self {
        Self { key_code, modifiers }
    }

    pub fn from_char(c: char) -> Self {
        let key_code = match c {
            'a'..='z' => XK_a + (c as KeyCode - 'a' as KeyCode),
            'A'..='Z' => XK_a + (c as KeyCode - 'A' as KeyCode),
            '0'..='9' => XK_0 + (c as KeyCode - '0' as KeyCode),
            ' ' => XK_space,
            _ => c as KeyCode,
        };
        Self { key_code, modifiers: 0 }
    }
}