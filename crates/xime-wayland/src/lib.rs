pub mod im_v1;
pub mod im_v2;

pub use im_v2::{
    Error as ErrorV2, InputMethodData, InputMethodState, Result as ResultV2, WaylandConnectionV2,
    ZwpInputMethodManagerV2, ZwpInputMethodV2,
};

pub use im_v1::{
    Error as ErrorV1, InputMethodV1Data, InputMethodV1State, Result as ResultV1,
    WaylandConnectionV1,
};

use std::os::unix::io::OwnedFd;
use std::os::unix::net::UnixStream;

use wayland_backend::client::Backend;
use wayland_client::globals::registry_queue_init;
use wayland_client::Connection;
use xime_ui::CandidateItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub serial: u32,
    pub time: u32,
    pub key: u32,
    pub pressed: bool,
}

/// 指针点击事件（候选栏 / 面板 surface 局部坐标）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerEvent {
    pub serial: u32,
    pub time: u32,
    pub x: i32,
    pub y: i32,
    /// true = 按下，false = 释放。
    pub pressed: bool,
    /// wl_pointer button code（BTN_LEFT = 272）。
    pub button: u32,
    /// 点击是否发生在菜单面板 surface 上。
    pub on_menu: bool,
}

/// Common backend interface shared by the v1 (KWin) and v2 (GNOME/wlroots)
/// input method implementations.
pub trait ImBackend {
    fn dispatch_events(&mut self) -> Result<(), String>;
    fn is_active(&self) -> bool;
    fn pop_key_events(&self) -> Vec<KeyEvent>;
    fn pop_pointer_events(&self) -> Vec<PointerEvent>;
    fn get_modifiers(&self) -> (u32, u32, u32, u32);
    fn get_keymap_pending(&self) -> Option<(OwnedFd, usize)>;
    fn forward_key(&self, serial: u32, time: u32, key: u32, pressed: bool);
    fn commit_string(&self, text: &str);
    fn set_preedit(&self, text: &str, cursor: i32);
    fn clear_preedit(&self);
    fn flush(&self) -> Result<(), String>;
    fn show_candidate_window(
        &mut self,
        candidates: &[CandidateItem],
        highlighted_index: usize,
        primary_color: (u8, u8, u8),
    ) -> Result<(), String>;
    /// 候选栏自然宽度（内容 + 菜单按钮），用于命中测试。
    fn candidate_width(&mut self, _candidates: &[CandidateItem]) -> u32 {
        200
    }
    fn hide_candidate_window(&mut self);
    /// 显示菜单面板（候选栏右侧按钮点击后展开的功能入口列表）。
    /// 面板内容含高亮入口（active_index = None 表示无高亮）。
    fn show_menu_panel(
        &mut self,
        active_index: Option<usize>,
        primary_color: (u8, u8, u8),
    ) -> Result<(), String>;
    fn hide_menu_panel(&mut self);
    fn show_root_window(
        &mut self,
        key: char,
        root: &str,
        primary_color: (u8, u8, u8),
    ) -> Result<(), String>;
    fn hide_root_window(&mut self);
    /// Recreate the input method object after the compositor reports it
    /// unavailable (e.g. GNOME lock screen). No-op on v1.
    fn handle_unavailable(&mut self) -> Result<(), String>;
}

impl ImBackend for im_v1::WaylandConnectionV1 {
    fn dispatch_events(&mut self) -> Result<(), String> {
        self.dispatch_events().map_err(|e| e.to_string())
    }

    fn is_active(&self) -> bool {
        self.get_state().state == im_v1::InputMethodV1State::Active
    }

    fn pop_key_events(&self) -> Vec<KeyEvent> {
        self.pop_key_events()
    }

    fn pop_pointer_events(&self) -> Vec<PointerEvent> {
        self.pop_pointer_events()
    }

    fn get_modifiers(&self) -> (u32, u32, u32, u32) {
        self.get_modifiers()
    }

    fn get_keymap_pending(&self) -> Option<(OwnedFd, usize)> {
        self.get_keymap_pending()
    }

    fn forward_key(&self, serial: u32, time: u32, key: u32, pressed: bool) {
        self.forward_key(serial, time, key, pressed)
    }

    fn commit_string(&self, text: &str) {
        self.commit_string(text)
    }

    fn set_preedit(&self, text: &str, cursor: i32) {
        self.set_preedit(text, cursor)
    }

    fn clear_preedit(&self) {
        self.clear_preedit()
    }

    fn flush(&self) -> Result<(), String> {
        self.flush().map_err(|e| e.to_string())
    }

    fn show_candidate_window(
        &mut self,
        candidates: &[CandidateItem],
        highlighted_index: usize,
        primary_color: (u8, u8, u8),
    ) -> Result<(), String> {
        self.show_candidate_window(candidates, highlighted_index, primary_color)
            .map_err(|e| e.to_string())
    }

    fn candidate_width(&mut self, candidates: &[CandidateItem]) -> u32 {
        self.candidate_width(candidates)
    }

    fn hide_candidate_window(&mut self) {
        self.hide_candidate_window()
    }

    fn show_menu_panel(
        &mut self,
        active_index: Option<usize>,
        primary_color: (u8, u8, u8),
    ) -> Result<(), String> {
        self.show_menu_panel(active_index, primary_color)
            .map_err(|e| e.to_string())
    }

    fn hide_menu_panel(&mut self) {
        self.hide_menu_panel()
    }

    fn show_root_window(
        &mut self,
        key: char,
        root: &str,
        primary_color: (u8, u8, u8),
    ) -> Result<(), String> {
        self.show_root_window(key, root, primary_color)
            .map_err(|e| e.to_string())
    }

    fn hide_root_window(&mut self) {
        self.hide_root_window()
    }

    fn handle_unavailable(&mut self) -> Result<(), String> {
        Ok(())
    }
}

impl ImBackend for im_v2::WaylandConnectionV2 {
    fn dispatch_events(&mut self) -> Result<(), String> {
        self.dispatch_events().map_err(|e| e.to_string())
    }

    fn is_active(&self) -> bool {
        self.get_state().state == im_v2::InputMethodState::Active
    }

    fn pop_key_events(&self) -> Vec<KeyEvent> {
        self.pop_key_events()
    }

    fn pop_pointer_events(&self) -> Vec<PointerEvent> {
        self.pop_pointer_events()
    }

    fn get_modifiers(&self) -> (u32, u32, u32, u32) {
        self.get_modifiers()
    }

    fn get_keymap_pending(&self) -> Option<(OwnedFd, usize)> {
        self.get_keymap_pending()
    }

    fn forward_key(&self, serial: u32, time: u32, key: u32, pressed: bool) {
        self.forward_key(serial, time, key, pressed)
    }

    fn commit_string(&self, text: &str) {
        self.commit_string(text)
    }

    fn set_preedit(&self, text: &str, cursor: i32) {
        self.set_preedit(text, cursor)
    }

    fn clear_preedit(&self) {
        self.clear_preedit()
    }

    fn flush(&self) -> Result<(), String> {
        self.flush().map_err(|e| e.to_string())
    }

    fn show_candidate_window(
        &mut self,
        candidates: &[CandidateItem],
        highlighted_index: usize,
        primary_color: (u8, u8, u8),
    ) -> Result<(), String> {
        self.show_candidate_window(candidates, highlighted_index, primary_color)
            .map_err(|e| e.to_string())
    }

    fn candidate_width(&mut self, candidates: &[CandidateItem]) -> u32 {
        self.candidate_width(candidates)
    }

    fn hide_candidate_window(&mut self) {
        self.hide_candidate_window()
    }

    fn show_menu_panel(
        &mut self,
        active_index: Option<usize>,
        primary_color: (u8, u8, u8),
    ) -> Result<(), String> {
        self.show_menu_panel(active_index, primary_color)
            .map_err(|e| e.to_string())
    }

    fn hide_menu_panel(&mut self) {
        self.hide_menu_panel()
    }

    fn show_root_window(
        &mut self,
        key: char,
        root: &str,
        primary_color: (u8, u8, u8),
    ) -> Result<(), String> {
        self.show_root_window(key, root, primary_color)
            .map_err(|e| e.to_string())
    }

    fn hide_root_window(&mut self) {
        self.hide_root_window()
    }

    fn handle_unavailable(&mut self) -> Result<(), String> {
        self.handle_unavailable().map_err(|e| e.to_string())
    }
}

/// Connect to the compositor via the given fd (KWin launcher handoff) and
/// select the best available input method backend: v1 (KWin 5.x) first, then
/// v2 (KWin 6.x, wlroots, GNOME).
pub fn connect_im_from_fd(fd: OwnedFd) -> Result<Box<dyn ImBackend>, String> {
    let stream = UnixStream::from(fd);
    let backend = Backend::connect(stream).map_err(|e| format!("Failed to connect: {e}"))?;
    let connection = Connection::from_backend(backend);
    connect_im_with_connection(connection)
}

/// Connect to the compositor via `$WAYLAND_DISPLAY` (standalone mode, e.g.
/// GNOME where there is no launcher) and select the best available backend.
pub fn connect_im_to_env() -> Result<Box<dyn ImBackend>, String> {
    let connection = Connection::connect_to_env().map_err(|e| format!("Failed to connect: {e}"))?;
    connect_im_with_connection(connection)
}

fn connect_im_with_connection(connection: Connection) -> Result<Box<dyn ImBackend>, String> {
    // Probe which input method protocol the compositor exposes. Each event
    // queue needs its own registry initialization; the probe queue is
    // discarded before the real one is created.
    let (has_v1, has_v2) = {
        let (globals, _probe_queue) = registry_queue_init::<im_v1::InputMethodV1Data>(&connection)
            .map_err(|e| format!("Failed to probe globals: {e}"))?;
        (
            globals
                .contents()
                .with_list(|l| l.iter().any(|g| g.interface == "zwp_input_method_v1")),
            globals.contents().with_list(|l| {
                l.iter()
                    .any(|g| g.interface == "zwp_input_method_manager_v2")
            }),
        )
    };

    if has_v1 {
        let (globals, event_queue) = registry_queue_init(&connection)
            .map_err(|e| format!("Failed to init registry: {e}"))?;
        let qh = event_queue.handle();
        let c =
            im_v1::WaylandConnectionV1::init_from_registry(connection, globals, event_queue, &qh)
                .map_err(|e| e.to_string())?;
        return Ok(Box::new(c));
    }

    if has_v2 {
        let (globals, event_queue) = registry_queue_init(&connection)
            .map_err(|e| format!("Failed to init registry: {e}"))?;
        let qh = event_queue.handle();
        let c =
            im_v2::WaylandConnectionV2::init_from_registry(connection, globals, event_queue, &qh)
                .map_err(|e| e.to_string())?;
        return Ok(Box::new(c));
    }

    Err("No input method protocol available on this display".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_method_v1_state_default() {
        let state = InputMethodV1State::default();
        assert_eq!(state, InputMethodV1State::Inactive);
    }

    #[test]
    fn test_input_method_v1_state_equality() {
        assert_eq!(InputMethodV1State::Inactive, InputMethodV1State::Inactive);
        assert_eq!(InputMethodV1State::Active, InputMethodV1State::Active);
        assert_ne!(InputMethodV1State::Inactive, InputMethodV1State::Active);
    }

    #[test]
    fn test_input_method_v1_state_debug() {
        let debug = format!("{:?}", InputMethodV1State::Active);
        assert_eq!(debug, "Active");
    }

    #[test]
    fn test_input_method_v2_state_default() {
        let state = InputMethodState::default();
        assert_eq!(state, InputMethodState::Inactive);
    }

    #[test]
    fn test_input_method_v2_state_equality() {
        assert_eq!(InputMethodState::Inactive, InputMethodState::Inactive);
        assert_eq!(InputMethodState::Active, InputMethodState::Active);
        assert_ne!(InputMethodState::Inactive, InputMethodState::Active);
    }

    #[test]
    fn test_input_method_v2_state_debug() {
        let debug = format!("{:?}", InputMethodState::Active);
        assert_eq!(debug, "Active");
    }

    #[test]
    fn test_input_method_v1_data_default() {
        let data = InputMethodV1Data::default();
        assert_eq!(data.state, InputMethodV1State::Inactive);
        assert_eq!(data.serial, 0);
        assert!(data.surrounding_text.is_none());
    }

    #[test]
    fn test_input_method_v2_data_default() {
        let data = InputMethodData::default();
        assert_eq!(data.state, InputMethodState::Inactive);
        assert_eq!(data.serial, 0);
        assert!(data.surrounding_text.is_none());
    }

    #[test]
    fn test_key_event_creation() {
        let event = KeyEvent {
            serial: 42,
            time: 1000,
            key: 30,
            pressed: true,
        };
        assert_eq!(event.serial, 42);
        assert_eq!(event.time, 1000);
        assert_eq!(event.key, 30);
        assert!(event.pressed);
    }

    #[test]
    fn test_key_event_release() {
        let event = KeyEvent {
            serial: 43,
            time: 1001,
            key: 30,
            pressed: false,
        };
        assert!(!event.pressed);
    }

    #[test]
    fn test_pointer_event_creation() {
        let event = PointerEvent {
            serial: 50,
            time: 2000,
            x: 320,
            y: 12,
            pressed: true,
            button: 272,
            on_menu: false,
        };
        assert_eq!(event.serial, 50);
        assert_eq!(event.x, 320);
        assert_eq!(event.y, 12);
        assert!(event.pressed);
        assert_eq!(event.button, 272);
        assert!(!event.on_menu);
    }

    #[test]
    fn test_pointer_event_release() {
        let event = PointerEvent {
            serial: 51,
            time: 2001,
            x: 320,
            y: 12,
            pressed: false,
            button: 272,
            on_menu: true,
        };
        assert!(!event.pressed);
        assert!(event.on_menu);
    }

    #[test]
    fn test_error_v1_display() {
        let err = ErrorV1::NoInputMethod;
        assert_eq!(format!("{}", err), "No input method available");
    }

    #[test]
    fn test_error_v1_no_seat() {
        let err = ErrorV1::NoSeat;
        assert_eq!(format!("{}", err), "No seat available");
    }

    #[test]
    fn test_error_v1_no_compositor() {
        let err = ErrorV1::NoCompositor;
        assert_eq!(format!("{}", err), "No compositor available");
    }

    #[test]
    fn test_error_v2_display() {
        let err = ErrorV2::NoSeat;
        assert_eq!(format!("{}", err), "No seat available");
    }

    #[test]
    fn test_error_v2_no_input_method_manager() {
        let err = ErrorV2::NoInputMethodManager;
        assert_eq!(format!("{}", err), "No input method manager available");
    }

    #[test]
    fn test_input_method_v1_data_clone() {
        let data = InputMethodV1Data::default();
        let cloned = data.clone();
        assert_eq!(cloned.state, InputMethodV1State::Inactive);
    }
}
