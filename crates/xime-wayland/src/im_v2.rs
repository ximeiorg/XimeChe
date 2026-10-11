use std::os::unix::io::{AsFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::slice;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, error};
use wayland_backend::client::Backend;
use wayland_client;
use wayland_client::globals::{GlobalList, GlobalListContents};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_callback::WlCallback;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_pointer::WlPointer;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_shm::WlShm;
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::protocol::*;
use wayland_client::{globals::registry_queue_init, Connection, EventQueue};
use wayland_client::{Dispatch, Proxy, QueueHandle};
use xime_ui::{
    grid_panel_height, list_panel_height, menu_panel_height, CandidateItem, IcedSurface, PanelGrid,
    PanelList, PanelPage, PanelTheme, PANEL_MIN_WIDTH,
};

use crate::{KeyEvent, PointerEvent};

pub mod __interfaces {
    use wayland_client::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("protocols/input-method-unstable-v2.xml");
}

use self::__interfaces::*;

wayland_scanner::generate_client_code!("protocols/input-method-unstable-v2.xml");

pub mod zwp_text_input_v3 {
    pub use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::*;
    pub mod __interfaces {
        pub use wayland_protocols::wp::text_input::zv3::client::__interfaces::*;
    }
}

pub mod virtual_keyboard {
    use wayland_client;
    use wayland_client::protocol::wl_seat;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/virtual-keyboard-unstable-v1.xml");
    }

    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/virtual-keyboard-unstable-v1.xml");

    pub use zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
    pub use zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;
}

pub use virtual_keyboard::{ZwpVirtualKeyboardManagerV1, ZwpVirtualKeyboardV1};
pub use zwp_input_method_keyboard_grab_v2::ZwpInputMethodKeyboardGrabV2;
pub use zwp_input_method_manager_v2::ZwpInputMethodManagerV2;
pub use zwp_input_method_v2::Event as ImEvent;
pub use zwp_input_method_v2::ZwpInputMethodV2;
pub use zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMethodState {
    #[default]
    Inactive,
    Active,
}

#[derive(Debug, Clone, Default)]
pub struct InputMethodData {
    pub serial: u32,
    pub state: InputMethodState,
    pub surrounding_text: Option<String>,
    pub surrounding_cursor: u32,
    pub surrounding_anchor: u32,
    pub content_hint: u32,
    pub content_purpose: u32,
    pub unavailable: bool,
    pub keyboard: Arc<Mutex<Option<ZwpInputMethodKeyboardGrabV2>>>,
    pub virtual_keyboard: Arc<Mutex<Option<ZwpVirtualKeyboardV1>>>,
    pub key_events: Arc<Mutex<Vec<KeyEvent>>>,
    pub pointer_events: Arc<Mutex<Vec<PointerEvent>>>,
    pub pointer_pos: Arc<Mutex<(f64, f64)>>,
    /// 面板展开视图（菜单网格/内容网格），影响候选栏增高与渲染。
    /// 当前面板页面（None = 收起）；数据（列表/网格）由 daemon 注入。
    pub panel_page: Option<PanelPage>,
    pub panel_list: PanelList,
    pub panel_grid: PanelGrid,
    pub modifiers: Arc<Mutex<(u32, u32, u32, u32)>>,
    pub keymap_pending: Arc<Mutex<Option<(OwnedFd, usize)>>>,
    /// 楔死探测：已发出未 ack 的 wl_display.sync 时刻（wl_callback Done 时
    /// 清空）。超时未 ack = 合成器侧连接楔死，dispatch 返回 Err 交给自愈。
    pub sync_outstanding: Arc<Mutex<Option<Instant>>>,
}

/// sync 探针发送间隔（每秒一次，代价可忽略）。
const SYNC_PROBE_INTERVAL: Duration = Duration::from_secs(1);
/// sync 超时判定：合成器正常应答是毫秒级，2 秒无 ack 视为连接楔死。
const SYNC_ACK_TIMEOUT: Duration = Duration::from_secs(2);

impl Dispatch<WlCallback, InputMethodData> for InputMethodData {
    fn event(
        state: &mut InputMethodData,
        _proxy: &WlCallback,
        event: <WlCallback as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
        if matches!(event, wl_callback::Event::Done { .. }) {
            if let Ok(mut pending) = state.sync_outstanding.lock() {
                *pending = None;
            }
            // done 事件本身是析构（wayland.xml：type="destructor"），无需发送 destroy
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<WlSeat, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &WlSeat,
        _event: <WlSeat as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodManagerV2, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &ZwpInputMethodManagerV2,
        _event: <ZwpInputMethodManagerV2 as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &ZwpVirtualKeyboardManagerV1,
        _event: <ZwpVirtualKeyboardManagerV1 as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &ZwpVirtualKeyboardV1,
        _event: <ZwpVirtualKeyboardV1 as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<WlPointer, InputMethodData> for InputMethodData {
    fn event(
        state: &mut InputMethodData,
        _proxy: &WlPointer,
        event: <WlPointer as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                serial: _,
                surface: _,
                surface_x,
                surface_y,
            } => {
                if let Ok(mut pos) = state.pointer_pos.lock() {
                    *pos = (surface_x, surface_y);
                }
            }
            wl_pointer::Event::Motion {
                time: _,
                surface_x,
                surface_y,
            } => {
                if let Ok(mut pos) = state.pointer_pos.lock() {
                    *pos = (surface_x, surface_y);
                }
            }
            wl_pointer::Event::Button {
                serial,
                time,
                button,
                state: button_state,
            } => {
                let pressed = matches!(
                    button_state,
                    wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed)
                );
                let pos = state.pointer_pos.lock().map(|p| *p).unwrap_or((0.0, 0.0));
                debug!(
                    "Pointer button: serial={}, button={}, pressed={}, pos=({:.0},{:.0})",
                    serial, button, pressed, pos.0, pos.1
                );
                if let Ok(mut events) = state.pointer_events.lock() {
                    events.push(PointerEvent {
                        serial,
                        time,
                        x: pos.0 as i32,
                        y: pos.1 as i32,
                        pressed,
                        button,
                        on_menu: false,
                        on_voice: false,
                    });
                }
            }
            _ => {
                debug!("Pointer event: {:?}", event);
            }
        }
    }
}

impl Dispatch<ZwpInputMethodV2, InputMethodData> for InputMethodData {
    fn event(
        state: &mut InputMethodData,
        proxy: &ZwpInputMethodV2,
        event: <ZwpInputMethodV2 as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        qhandle: &QueueHandle<InputMethodData>,
    ) {
        match event {
            ImEvent::Activate => {
                debug!("zwp_input_method_v2 ACTIVATE event received");
                state.state = InputMethodState::Active;
                let grab = proxy.grab_keyboard(qhandle, state.clone());
                if let Ok(mut kb) = state.keyboard.lock() {
                    *kb = Some(grab);
                    debug!("Grabbed keyboard (v2) successfully");
                }
            }
            ImEvent::Deactivate => {
                debug!("zwp_input_method_v2 DEACTIVATE event received");
                state.state = InputMethodState::Inactive;
                if let Ok(mut kb) = state.keyboard.lock() {
                    if let Some(grab) = kb.take() {
                        grab.release();
                    }
                }
            }
            ImEvent::SurroundingText {
                text,
                cursor,
                anchor,
            } => {
                debug!(
                    "Input method SURROUNDING_TEXT: cursor={}, anchor={}, len={}",
                    cursor,
                    anchor,
                    text.len()
                );
                state.surrounding_text = Some(text);
                state.surrounding_cursor = cursor;
                state.surrounding_anchor = anchor;
            }
            ImEvent::TextChangeCause { cause } => {
                debug!("Input method TEXT_CHANGE_CAUSE: {:?}", cause);
            }
            ImEvent::ContentType { hint, purpose } => {
                let hint: u32 = hint.into();
                let purpose: u32 = purpose.into();
                debug!(
                    "Input method CONTENT_TYPE: hint={}, purpose={}",
                    hint, purpose
                );
                state.content_hint = hint;
                state.content_purpose = purpose;
            }
            ImEvent::Done => {
                // v2 `done` carries no serial; the commit serial must equal
                // the number of done events already received (fcitx5 does
                // the same with a counter).
                state.serial += 1;
                debug!("Input method DONE: serial={}", state.serial);
            }
            ImEvent::Unavailable => {
                debug!("Input method UNAVAILABLE (e.g. lock screen)");
                state.unavailable = true;
                state.state = InputMethodState::Inactive;
                if let Ok(mut kb) = state.keyboard.lock() {
                    *kb = None;
                }
            }
        }
    }
}

impl Dispatch<ZwpInputMethodKeyboardGrabV2, InputMethodData> for InputMethodData {
    fn event(
        state: &mut InputMethodData,
        _proxy: &ZwpInputMethodKeyboardGrabV2,
        event: <ZwpInputMethodKeyboardGrabV2 as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
        match event {
            zwp_input_method_keyboard_grab_v2::Event::Keymap { format, fd, size } => {
                debug!(
                    "Grab KEYMAP event received, format={:?}, size={}",
                    format, size
                );
                if let Ok(vk_guard) = state.virtual_keyboard.lock() {
                    if let Some(vk) = vk_guard.as_ref() {
                        if let Ok(clone) = fd.try_clone() {
                            let format: u32 = format.into();
                            vk.keymap(format, clone.as_fd(), size);
                        }
                    }
                }
                if let Ok(mut keymap) = state.keymap_pending.lock() {
                    *keymap = Some((fd, size as usize));
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::Key {
                serial,
                time,
                key,
                state: key_state,
            } => {
                let pressed = matches!(
                    key_state,
                    wayland_client::WEnum::Value(
                        wayland_client::protocol::wl_keyboard::KeyState::Pressed
                    )
                );
                debug!(
                    "Grab KEY event: serial={}, key={}, pressed={}",
                    serial, key, pressed
                );
                if let Ok(mut events) = state.key_events.lock() {
                    events.push(KeyEvent {
                        serial,
                        time,
                        key,
                        pressed,
                    });
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::Modifiers {
                serial: _,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } => {
                debug!(
                    "Grab MODIFIERS: depressed={}, latched={}, locked={}, group={}",
                    mods_depressed, mods_latched, mods_locked, group
                );
                if let Ok(mut mods) = state.modifiers.lock() {
                    *mods = (mods_depressed, mods_latched, mods_locked, group);
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::RepeatInfo { rate, delay } => {
                debug!("Grab REPEAT_INFO: rate={}, delay={}", rate, delay);
            }
        }
    }
}

impl Dispatch<ZwpInputPopupSurfaceV2, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &ZwpInputPopupSurfaceV2,
        event: <ZwpInputPopupSurfaceV2 as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
        match event {
            zwp_input_popup_surface_v2::Event::TextInputRectangle {
                x,
                y,
                width,
                height,
            } => {
                debug!(
                    "Popup surface text input rectangle: x={}, y={}, w={}, h={}",
                    x, y, width, height
                );
            }
        }
    }
}

impl Dispatch<WlCompositor, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &WlCompositor,
        _event: <WlCompositor as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<WlShm, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &WlShm,
        _event: <WlShm as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<WlSurface, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &WlSurface,
        _event: <WlSurface as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<WlBuffer, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &WlBuffer,
        _event: <WlBuffer as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

impl Dispatch<WlShmPool, InputMethodData> for InputMethodData {
    fn event(
        _state: &mut InputMethodData,
        _proxy: &WlShmPool,
        _event: <WlShmPool as Proxy>::Event,
        _data: &InputMethodData,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodData>,
    ) {
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to connect to Wayland display: {0}")]
    ConnectFailed(String),

    #[error("Failed to get global: {0}")]
    GlobalNotFound(String),

    #[error("Failed to bind interface: {0}")]
    BindFailed(String),

    #[error("No input method manager available")]
    NoInputMethodManager,

    #[error("No virtual keyboard manager available")]
    NoVirtualKeyboardManager,

    #[error("No input method available")]
    NoInputMethod,

    #[error("No compositor available")]
    NoCompositor,

    #[error("No SHM available")]
    NoShm,

    #[error("No seat available")]
    NoSeat,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct WaylandConnectionV2 {
    connection: Connection,
    event_queue: EventQueue<InputMethodData>,
    state: InputMethodData,
    seat: Option<WlSeat>,
    // 保持 proxy 存活以接收指针事件；事件经 state.pointer_events 收集。
    #[allow(dead_code)]
    pointer: Option<WlPointer>,
    input_method_manager: Option<ZwpInputMethodManagerV2>,
    virtual_keyboard_manager: Option<ZwpVirtualKeyboardManagerV1>,
    input_method: Option<ZwpInputMethodV2>,
    input_popup_surface: Option<ZwpInputPopupSurfaceV2>,
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    candidate_surface: Option<WlSurface>,
    current_buffer: Option<WlBuffer>,
    current_pool: Option<WlShmPool>,
    renderer: Option<IcedSurface>,
    /// 上次 sync 探针发送时刻（楔死探测节流）。
    last_sync_probe: Instant,
}

impl WaylandConnectionV2 {
    pub fn connect() -> Result<Self> {
        let connection =
            Connection::connect_to_env().map_err(|e| Error::ConnectFailed(e.to_string()))?;

        Self::init_with_connection(connection)
    }

    pub fn connect_from_fd(fd: OwnedFd) -> Result<Self> {
        let stream = UnixStream::from(fd);
        let backend = Backend::connect(stream).map_err(|e| Error::ConnectFailed(e.to_string()))?;
        let connection = Connection::from_backend(backend);

        Self::init_with_connection(connection)
    }

    pub fn init_with_connection(connection: Connection) -> Result<Self> {
        let (globals, event_queue) =
            registry_queue_init(&connection).map_err(|e| Error::ConnectFailed(e.to_string()))?;

        let qh = event_queue.handle();
        Self::init_from_registry(connection, globals, event_queue, &qh)
    }

    pub fn init_from_registry(
        connection: Connection,
        globals: GlobalList,
        event_queue: EventQueue<InputMethodData>,
        qh: &QueueHandle<InputMethodData>,
    ) -> Result<Self> {
        let state = InputMethodData::default();

        let seat: Option<WlSeat> = globals.bind(qh, 1..=8, state.clone()).ok();

        let pointer: Option<WlPointer> = seat.as_ref().map(|s| s.get_pointer(qh, state.clone()));

        let compositor: Option<WlCompositor> = globals.bind(qh, 1..=4, state.clone()).ok();

        let shm: Option<WlShm> = globals.bind(qh, 1..=1, state.clone()).ok();

        let input_method_manager: Option<ZwpInputMethodManagerV2> =
            globals.bind(qh, 1..=1, state.clone()).ok();

        let virtual_keyboard_manager: Option<ZwpVirtualKeyboardManagerV1> =
            globals.bind(qh, 1..=1, state.clone()).ok();

        Ok(Self {
            connection,
            event_queue,
            state,
            seat,
            pointer,
            input_method_manager,
            virtual_keyboard_manager,
            input_method: None,
            input_popup_surface: None,
            compositor,
            shm,
            candidate_surface: None,
            current_buffer: None,
            current_pool: None,
            renderer: None,
            last_sync_probe: Instant::now(),
        })
    }

    pub fn has_zwp_input_method_manager_v2_global(&self) -> bool {
        self.input_method_manager.is_some()
    }

    pub fn get_seat(&self) -> Result<&WlSeat> {
        self.seat.as_ref().ok_or(Error::NoSeat)
    }

    pub fn get_input_method_manager(&self) -> Result<&ZwpInputMethodManagerV2> {
        self.input_method_manager
            .as_ref()
            .ok_or(Error::NoInputMethodManager)
    }

    pub fn get_input_method(&self) -> Result<&ZwpInputMethodV2> {
        self.input_method.as_ref().ok_or(Error::NoInputMethod)
    }

    pub fn create_input_method(&mut self) -> Result<()> {
        let seat = self.get_seat()?.clone();
        let manager = self.get_input_method_manager()?.clone();
        let vk_manager = self
            .virtual_keyboard_manager
            .as_ref()
            .ok_or(Error::NoVirtualKeyboardManager)?
            .clone();

        let qh = self.event_queue.handle();

        let input_method = manager.get_input_method(&seat, &qh, self.state.clone());
        self.input_method = Some(input_method);

        let virtual_keyboard = vk_manager.create_virtual_keyboard(&seat, &qh, self.state.clone());
        if let Ok(mut vk) = self.state.virtual_keyboard.lock() {
            *vk = Some(virtual_keyboard);
        }

        self.sync_roundtrip()?;

        debug!("Created input method v2 + virtual keyboard");
        Ok(())
    }

    /// Recreate the input method object after an `unavailable` event (e.g.
    /// GNOME lock screen). The old object becomes inert and must be destroyed.
    pub fn handle_unavailable(&mut self) -> Result<()> {
        if !self.state.unavailable {
            return Ok(());
        }
        debug!("Recreating input method after unavailable");
        self.state.unavailable = false;
        self.state.state = InputMethodState::Inactive;

        if let Some(im) = self.input_method.take() {
            im.destroy();
        }
        // 候选窗 popup 挂在旧 input_method 上，IM 对象销毁后它随之失效；
        // 不清掉的话 show_candidate_window 因 candidate_surface.is_some()
        // 而跳过重建，解锁/重建后候选窗永远不再出现。
        self.candidate_surface = None;
        self.input_popup_surface = None;

        let seat = self.get_seat()?.clone();
        let manager = self.get_input_method_manager()?.clone();
        let qh = self.event_queue.handle();
        let input_method = manager.get_input_method(&seat, &qh, self.state.clone());
        self.input_method = Some(input_method);

        self.sync_roundtrip()?;
        Ok(())
    }

    pub fn dispatch_events(&mut self) -> Result<()> {
        // 非阻塞事件泵 + 合成器健康探测（与 im_v1 同款）：阻塞 roundtrip 在
        // 合成器停止应答时会永久挂起主循环，楔死探测超时返回 Err 交给
        // daemon 的 HealScheduler 走既有恢复链。
        self.flush()?;

        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;

        // 非阻塞读：socket 可读才 read_events（0 超时 = 纯探测）。
        if let Some(guard) = self.event_queue.prepare_read() {
            let backend = self.connection.backend();
            let fd = backend.poll_fd();
            let mut fds = [nix::poll::PollFd::new(fd, nix::poll::PollFlags::POLLIN)];
            match nix::poll::poll(&mut fds, nix::poll::PollTimeout::ZERO) {
                Ok(n)
                    if n > 0
                        && fds[0]
                            .revents()
                            .is_some_and(|f| f.contains(nix::poll::PollFlags::POLLIN)) =>
                {
                    guard
                        .read()
                        .map_err(|e| Error::Io(std::io::Error::other(e)))?;
                    self.event_queue
                        .dispatch_pending(&mut self.state)
                        .map_err(|e| Error::Io(std::io::Error::other(e)))?;
                }
                Ok(_) => {}
                Err(e) => return Err(Error::Io(std::io::Error::from(e))),
            }
        }

        // 楔死探测：有未 ack 的 sync 且超时 → 连接已死；到期则补发探针。
        let outstanding = self.state.sync_outstanding.lock().ok().and_then(|p| *p);
        match outstanding {
            Some(sent_at) => {
                if sent_at.elapsed() > SYNC_ACK_TIMEOUT {
                    error!("Compositor sync not acked in {SYNC_ACK_TIMEOUT:?}, connection wedged");
                    return Err(Error::Io(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "wayland sync probe not acked (compositor wedged)",
                    )));
                }
            }
            None => {
                let now = Instant::now();
                if now.duration_since(self.last_sync_probe) >= SYNC_PROBE_INTERVAL {
                    self.connection
                        .display()
                        .sync(&self.event_queue.handle(), self.state.clone());
                    if let Ok(mut pending) = self.state.sync_outstanding.lock() {
                        *pending = Some(now);
                    }
                    self.last_sync_probe = now;
                }
            }
        }

        Ok(())
    }

    pub fn dispatch_pending(&mut self) -> Result<()> {
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        Ok(())
    }

    pub fn sync_roundtrip(&mut self) -> Result<()> {
        self.event_queue
            .roundtrip(&mut self.state)
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        Ok(())
    }

    pub fn get_state(&self) -> &InputMethodData {
        &self.state
    }

    pub fn pop_key_events(&self) -> Vec<KeyEvent> {
        if let Ok(mut events) = self.state.key_events.lock() {
            let result = events.clone();
            events.clear();
            result
        } else {
            Vec::new()
        }
    }

    pub fn pop_pointer_events(&self) -> Vec<PointerEvent> {
        if let Ok(mut events) = self.state.pointer_events.lock() {
            let result = events.clone();
            events.clear();
            result
        } else {
            Vec::new()
        }
    }

    pub fn get_modifiers(&self) -> (u32, u32, u32, u32) {
        if let Ok(mods) = self.state.modifiers.lock() {
            *mods
        } else {
            (0, 0, 0, 0)
        }
    }

    pub fn get_keymap_pending(&self) -> Option<(OwnedFd, usize)> {
        if let Ok(mut keymap) = self.state.keymap_pending.lock() {
            keymap.take()
        } else {
            None
        }
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn event_queue(&mut self) -> &mut EventQueue<InputMethodData> {
        &mut self.event_queue
    }

    /// Forward an unhandled key to the focused client via the virtual keyboard
    /// protocol. v2 has no `forward_key` request on the grab, this is how
    /// fcitx5 does it on GNOME and wlroots compositors.
    pub fn forward_key(&self, _serial: u32, time: u32, key: u32, pressed: bool) {
        let key_state: u32 = if pressed { 1 } else { 0 };
        if let Ok(vk_guard) = self.state.virtual_keyboard.lock() {
            if let Some(vk) = vk_guard.as_ref() {
                vk.key(time, key, key_state);
            }
        }
    }

    /// v2 uses a double-buffered state: requests are applied on `commit`,
    /// whose serial must match the last `done` event received.
    pub fn commit_string(&self, text: &str) {
        if let Some(im) = &self.input_method {
            im.commit_string(text.to_string());
            im.commit(self.state.serial);
        }
    }

    pub fn set_preedit(&self, text: &str, cursor: i32) {
        if let Some(im) = &self.input_method {
            im.set_preedit_string(text.to_string(), cursor, cursor);
            im.commit(self.state.serial);
        }
    }

    pub fn clear_preedit(&self) {
        if let Some(im) = &self.input_method {
            im.set_preedit_string(String::new(), 0, 0);
            im.commit(self.state.serial);
        }
    }

    pub fn flush(&self) -> Result<()> {
        self.connection
            .flush()
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;
        Ok(())
    }

    pub fn create_candidate_surface(&mut self) -> Result<()> {
        let compositor = self.compositor.as_ref().ok_or(Error::NoCompositor)?;
        let input_method = self.get_input_method()?.clone();

        let qh = self.event_queue.handle();

        let surface = compositor.create_surface(&qh, self.state.clone());

        let popup_surface = input_method.get_input_popup_surface(&surface, &qh, self.state.clone());

        self.candidate_surface = Some(surface);
        self.input_popup_surface = Some(popup_surface);

        self.connection
            .flush()
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;

        debug!("Created input popup surface (v2)");
        Ok(())
    }

    pub fn show_candidate_window(
        &mut self,
        candidates: &[CandidateItem],
        highlighted_index: usize,
        theme: &PanelTheme,
    ) -> Result<()> {
        // 面板展开视图决定增高与宽度（候选栏高度随主题字号自适应）
        let page = self.state.panel_page;
        let panel_height = match page {
            Some(PanelPage::Menu) => menu_panel_height(),
            Some(PanelPage::Clipboard | PanelPage::QuickSend) => list_panel_height(),
            Some(_) => {
                let tabs = self.state.panel_grid.tab_count();
                grid_panel_height(tabs, self.state.panel_grid.has_pager)
            }
            None => 0,
        };
        // 面板区与候选栏之间有 PANEL_GAP（对齐 XimeYao）：面板展开时
        // buffer 高度多出这 4px 缝隙，命中换算同步（daemon 指针事件）。
        let height = theme.bar_height()
            + if panel_height > 0 {
                xime_ui::menu::PANEL_GAP + panel_height
            } else {
                0
            };

        // Take surface out of self for width measurement and drawing
        let mut surface = self.renderer.take().unwrap_or_default();
        // measure_candidates 已包含右侧菜单按钮宽度
        let measured = surface.measure_candidates(candidates, theme);
        // 面板宽度跟随候选栏，但不小于面板最小宽度（对齐 XimeYao）。
        let width = measured.max(PANEL_MIN_WIDTH);

        if self.candidate_surface.is_none() {
            self.create_candidate_surface()?;
        }

        if let Some(buffer) = self.current_buffer.take() {
            buffer.destroy();
        }
        if let Some(pool) = self.current_pool.take() {
            pool.destroy();
        }

        // Clone needed resources out of self before mutable operations
        let shm = self.shm.clone().ok_or(Error::NoShm)?;
        let surface_obj = self.candidate_surface.clone().unwrap();
        let qh = self.event_queue.handle();

        let stride = width * 4;
        let shm_size = stride * height;

        // Create SHM pool (doesn't borrow self since we pass cloned resources)
        let fd = Self::create_anonymous_file(shm_size)?;
        let pool = shm.create_pool(fd.as_fd(), shm_size as i32, &qh, self.state.clone());
        self.current_pool = Some(pool.clone());

        // mmap SHM buffer and draw with iced
        let buf_size = (width * height * 4) as usize;
        let size_nonzero = std::num::NonZero::new(buf_size).expect("size should be non-zero");

        let ptr = unsafe {
            nix::sys::mman::mmap(
                None,
                size_nonzero,
                nix::sys::mman::ProtFlags::PROT_READ | nix::sys::mman::ProtFlags::PROT_WRITE,
                nix::sys::mman::MapFlags::MAP_SHARED,
                &fd,
                0,
            )
            .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?
        };

        let pixels: &mut [u8] =
            unsafe { slice::from_raw_parts_mut(ptr.as_ptr() as *mut u8, buf_size) };

        // 统一 iced 绘制：候选栏 + 菜单按钮 + 展开视图（菜单网格/内容网格）
        surface.draw_panel(
            pixels,
            width,
            height,
            candidates,
            highlighted_index,
            theme,
            page,
            &self.state.panel_list.clone(),
            &self.state.panel_grid.clone(),
        );

        unsafe {
            nix::sys::mman::munmap(ptr, buf_size)
                .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;
        }

        // Put surface back into self
        self.renderer = Some(surface);

        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            &qh,
            self.state.clone(),
        );
        self.current_buffer = Some(buffer.clone());

        // 候选栏（buffer 顶部候选栏高度）锚定光标，菜单面板在其下方展开。
        surface_obj.attach(Some(&buffer), 0, 0);
        surface_obj.damage_buffer(0, 0, width as i32, height as i32);
        surface_obj.commit();

        Ok(())
    }

    pub fn hide_candidate_window(&mut self) {
        if let Some(surface) = &self.candidate_surface {
            surface.attach(None::<&WlBuffer>, 0, 0);
            surface.commit();
        }
    }

    /// 候选栏自然宽度（内容 + 菜单按钮），用于命中测试。
    pub fn candidate_width(&mut self, candidates: &[CandidateItem], theme: &PanelTheme) -> u32 {
        let mut surface = self.renderer.take().unwrap_or_default();
        let width = surface.measure_candidates(candidates, theme);
        self.renderer = Some(surface);
        width
    }

    /// 打开面板页面：仅设置页标志与注入数据（渲染随下一次
    /// show_candidate_window 生效；高度按页面计算）。
    pub fn show_panel(
        &mut self,
        page: PanelPage,
        list: &PanelList,
        grid: &PanelGrid,
    ) -> Result<()> {
        self.state.panel_page = Some(page);
        self.state.panel_list = list.clone();
        self.state.panel_grid = grid.clone();
        debug!("Panel page set to {page:?} (rendered on next candidate refresh)");
        Ok(())
    }

    pub fn hide_panel(&mut self) {
        self.state.panel_page = None;
        self.state.panel_list = PanelList::default();
        self.state.panel_grid = PanelGrid::default();
    }

    /// Show a single key root display window
    /// Displays "a: 工匚戈艹廿龷七弋戈" in a small popup
    pub fn show_root_window(&mut self, key: char, root: &str, theme: &PanelTheme) -> Result<()> {
        let mut surface = self.renderer.take().unwrap_or_default();
        let width = surface.measure_root(key, root, theme);
        let height = theme.bar_height();

        // Use candidate_surface to display root (same surface, different content)
        let shm = self.shm.clone().ok_or(Error::NoShm)?;
        let surface_obj = self
            .candidate_surface
            .clone()
            .ok_or(Error::Io(std::io::Error::other("No candidate surface")))?;

        // Destroy old buffer and pool
        if let Some(buffer) = self.current_buffer.take() {
            buffer.destroy();
        }
        if let Some(pool) = self.current_pool.take() {
            pool.destroy();
        }

        let qh = self.event_queue.handle();

        let stride = width * 4;
        let size = stride * height;

        let fd = Self::create_anonymous_file(size)?;
        let pool = shm.create_pool(fd.as_fd(), size as i32, &qh, self.state.clone());
        self.current_pool = Some(pool.clone());

        self.draw_root(&fd, width, height, key, root, theme, &mut surface)?;
        self.renderer = Some(surface);

        let buffer = pool.create_buffer(
            0,
            width as i32,
            height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            &qh,
            self.state.clone(),
        );
        self.current_buffer = Some(buffer.clone());

        surface_obj.attach(Some(&buffer), 0, 0);
        surface_obj.damage_buffer(0, 0, width as i32, height as i32);
        surface_obj.commit();

        self.connection
            .flush()
            .map_err(|e| Error::Io(std::io::Error::other(e)))?;

        Ok(())
    }

    pub fn hide_root_window(&mut self) {
        // No need to hide, main loop will restore candidate display
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_root(
        &self,
        fd: &OwnedFd,
        width: u32,
        height: u32,
        key: char,
        root: &str,
        theme: &PanelTheme,
        surface: &mut IcedSurface,
    ) -> Result<()> {
        let size = (width * height * 4) as usize;
        let size_nonzero = std::num::NonZero::new(size).expect("size should be non-zero");

        let ptr = unsafe {
            nix::sys::mman::mmap(
                None,
                size_nonzero,
                nix::sys::mman::ProtFlags::PROT_READ | nix::sys::mman::ProtFlags::PROT_WRITE,
                nix::sys::mman::MapFlags::MAP_SHARED,
                fd.as_fd(),
                0,
            )
            .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?
        };

        let pixels: &mut [u8] = unsafe { slice::from_raw_parts_mut(ptr.as_ptr() as *mut u8, size) };

        surface.draw_root(pixels, width, height, key, root, theme);

        unsafe {
            nix::sys::mman::munmap(ptr, size)
                .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;
        }

        Ok(())
    }

    fn create_anonymous_file(size: u32) -> Result<OwnedFd> {
        // memfd：纯匿名内存，不落文件系统（/tmp tmpfs 满时写页会 SIGBUS）。
        if let Ok(fd) = nix::sys::memfd::memfd_create(
            c"xime-shm",
            nix::sys::memfd::MemFdCreateFlag::MFD_CLOEXEC,
        ) {
            let file = std::fs::File::from(fd);
            file.set_len(size as u64)?;
            return Ok(file.into());
        }

        // 回退：老内核无 memfd 时用 O_TMPFILE。
        let fd = nix::fcntl::open(
            &std::env::temp_dir(),
            nix::fcntl::OFlag::O_TMPFILE | nix::fcntl::OFlag::O_RDWR | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )
        .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;

        let owned_fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let file = std::fs::File::from(owned_fd);
        file.set_len(size as u64)?;

        Ok(file.into())
    }
}
