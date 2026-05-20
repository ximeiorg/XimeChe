use std::os::unix::io::{AsFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::slice;
use std::sync::{Arc, Mutex};
use tracing::debug;
use wayland_backend::client::Backend;
use wayland_client;
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_keyboard::WlKeyboard;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_shm::WlShm;
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::protocol::*;
use wayland_client::{event_created_child, globals::registry_queue_init, Connection, EventQueue};
use wayland_client::{Dispatch, Proxy, QueueHandle};
use xime_ui::calculate_root_width;
use xime_ui::draw_candidates_to_buffer;
use xime_ui::draw_root_to_buffer;
use xime_ui::CandidateItem;

pub mod __interfaces {
    use wayland_client::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("protocols/input-method-unstable-v1.xml");
}

use self::__interfaces::*;

wayland_scanner::generate_client_code!("protocols/input-method-unstable-v1.xml");

pub use zwp_input_method_context_v1::Event as ContextEvent;
pub use zwp_input_method_context_v1::ZwpInputMethodContextV1;
pub use zwp_input_method_v1::Event as ImV1Event;
pub use zwp_input_method_v1::ZwpInputMethodV1;
pub use zwp_input_panel_surface_v1::ZwpInputPanelSurfaceV1;
pub use zwp_input_panel_v1::ZwpInputPanelV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMethodV1State {
    #[default]
    Inactive,
    Active,
}

#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub serial: u32,
    pub time: u32,
    pub key: u32,
    pub pressed: bool,
}

#[derive(Clone, Default)]
pub struct InputMethodV1Data {
    pub serial: u32,
    pub state: InputMethodV1State,
    pub surrounding_text: Option<String>,
    pub surrounding_cursor: u32,
    pub surrounding_anchor: u32,
    pub content_hint: u32,
    pub content_purpose: u32,
    pub context: Arc<Mutex<Option<ZwpInputMethodContextV1>>>,
    pub keyboard: Arc<Mutex<Option<WlKeyboard>>>,
    pub key_events: Arc<Mutex<Vec<KeyEvent>>>,
    pub modifiers: Arc<Mutex<(u32, u32, u32, u32)>>,
    pub keymap_pending: Arc<Mutex<Option<(OwnedFd, usize)>>>,
}

impl Dispatch<WlRegistry, GlobalListContents> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
    }
}

impl Dispatch<WlSeat, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &WlSeat,
        _event: <WlSeat as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
    }
}

impl Dispatch<WlOutput, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &WlOutput,
        _event: <WlOutput as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodV1, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        state: &mut InputMethodV1Data,
        _proxy: &ZwpInputMethodV1,
        event: <ZwpInputMethodV1 as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
        match event {
            ImV1Event::Activate { id } => {
                debug!("zwp_input_method_v1 ACTIVATE event received");
                state.state = InputMethodV1State::Active;
                let keyboard = id.grab_keyboard(qhandle, state.clone());
                if let Ok(mut kb) = state.keyboard.lock() {
                    *kb = Some(keyboard);
                    debug!("Grabbed keyboard successfully");
                }
                if let Ok(mut ctx) = state.context.lock() {
                    *ctx = Some(id);
                    debug!("Stored context successfully");
                }
            }
            ImV1Event::Deactivate { context: _ } => {
                debug!("zwp_input_method_v1 DEACTIVATE event received");
                state.state = InputMethodV1State::Inactive;
                if let Ok(mut ctx) = state.context.lock() {
                    *ctx = None;
                }
                if let Ok(mut kb) = state.keyboard.lock() {
                    *kb = None;
                }
            }
        }
    }

    event_created_child!(InputMethodV1Data, ZwpInputMethodV1, [
        zwp_input_method_v1::EVT_ACTIVATE_OPCODE => (ZwpInputMethodContextV1, InputMethodV1Data::default()),
    ]);
}

impl Dispatch<ZwpInputMethodContextV1, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        state: &mut InputMethodV1Data,
        _proxy: &ZwpInputMethodContextV1,
        event: <ZwpInputMethodContextV1 as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
        match event {
            ContextEvent::SurroundingText {
                text,
                cursor,
                anchor,
            } => {
                state.surrounding_text = Some(text);
                state.surrounding_cursor = cursor;
                state.surrounding_anchor = anchor;
            }
            ContextEvent::ContentType { hint, purpose } => {
                state.content_hint = hint;
                state.content_purpose = purpose;
            }
            ContextEvent::CommitState { serial } => {
                state.serial = serial;
            }
            ContextEvent::Reset => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpInputPanelV1, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &ZwpInputPanelV1,
        _event: <ZwpInputPanelV1 as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
    }
}

impl Dispatch<WlKeyboard, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        state: &mut InputMethodV1Data,
        _proxy: &WlKeyboard,
        event: <WlKeyboard as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
        match event {
            wl_keyboard::Event::Keymap {
                format: _,
                fd,
                size,
            } => {
                debug!("Keymap event received, size={}", size);
                if let Ok(mut keymap) = state.keymap_pending.lock() {
                    *keymap = Some((fd, size as usize));
                }
            }
            wl_keyboard::Event::Key {
                serial,
                time,
                key,
                state: key_state,
            } => {
                let pressed = matches!(
                    key_state,
                    wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed)
                );
                debug!(
                    "Key event: serial={}, key={}, pressed={}",
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
            wl_keyboard::Event::Modifiers {
                serial: _,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } => {
                if let Ok(mut mods) = state.modifiers.lock() {
                    *mods = (mods_depressed, mods_latched, mods_locked, group);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlCompositor, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &WlCompositor,
        _event: <WlCompositor as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
    }
}

impl Dispatch<WlShm, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &WlShm,
        _event: <WlShm as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
    }
}

impl Dispatch<WlSurface, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &WlSurface,
        event: <WlSurface as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
        match event {
            wl_surface::Event::Enter { output: _ } => {
                debug!("Surface enter output");
            }
            wl_surface::Event::Leave { output: _ } => {
                debug!("Surface leave output");
            }
            _ => {}
        }
    }
}

impl Dispatch<WlBuffer, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &WlBuffer,
        _event: <WlBuffer as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
    }
}

impl Dispatch<WlShmPool, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &WlShmPool,
        _event: <WlShmPool as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
    ) {
    }
}

impl Dispatch<ZwpInputPanelSurfaceV1, InputMethodV1Data> for InputMethodV1Data {
    fn event(
        _state: &mut InputMethodV1Data,
        _proxy: &ZwpInputPanelSurfaceV1,
        _event: <ZwpInputPanelSurfaceV1 as Proxy>::Event,
        _data: &InputMethodV1Data,
        _conn: &wayland_client::Connection,
        _qhandle: &QueueHandle<InputMethodV1Data>,
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

    #[error("No input method available")]
    NoInputMethod,

    #[error("No input panel available")]
    NoInputPanel,

    #[error("No output available")]
    NoOutput,

    #[error("No compositor available")]
    NoCompositor,

    #[error("No SHM available")]
    NoShm,

    #[error("No seat available")]
    NoSeat,

    #[error("No active context")]
    NoContext,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct WaylandConnectionV1 {
    connection: Connection,
    event_queue: EventQueue<InputMethodV1Data>,
    state: InputMethodV1Data,
    seat: Option<WlSeat>,
    input_method: Option<ZwpInputMethodV1>,
    input_panel: Option<ZwpInputPanelV1>,
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    output: Option<WlOutput>,
    has_v1_global: bool,
    panel_surface: Option<ZwpInputPanelSurfaceV1>,
    candidate_surface: Option<WlSurface>,
    current_buffer: Option<WlBuffer>,
    current_pool: Option<WlShmPool>,
}

impl WaylandConnectionV1 {
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
        let state = InputMethodV1Data::default();

        let seat: Option<WlSeat> = globals.bind(&qh, 1..=8, state.clone()).ok();

        let compositor: Option<WlCompositor> = globals.bind(&qh, 1..=4, state.clone()).ok();

        let shm: Option<WlShm> = globals.bind(&qh, 1..=1, state.clone()).ok();

        let input_method: Option<ZwpInputMethodV1> = globals.bind(&qh, 1..=1, state.clone()).ok();

        let input_panel: Option<ZwpInputPanelV1> = globals.bind(&qh, 1..=1, state.clone()).ok();

        let output: Option<WlOutput> = globals.bind(&qh, 1..=1, state.clone()).ok();

        // Check if v1 global exists by checking bind result
        let has_v1_global = input_method.is_some() || input_panel.is_some();

        Ok(Self {
            connection,
            event_queue,
            state,
            seat,
            input_method,
            input_panel,
            compositor,
            shm,
            output,
            has_v1_global,
            panel_surface: None,
            candidate_surface: None,
            current_buffer: None,
            current_pool: None,
        })
    }

    pub fn has_zwp_input_method_v1_global(&self) -> bool {
        self.has_v1_global
    }

    pub fn get_seat(&self) -> Result<&WlSeat> {
        self.seat.as_ref().ok_or(Error::NoSeat)
    }

    pub fn get_input_method(&self) -> Result<&ZwpInputMethodV1> {
        self.input_method.as_ref().ok_or(Error::NoInputMethod)
    }

    pub fn dispatch_events(&mut self) -> Result<()> {
        // Blocking dispatch - wait for at least one event
        self.event_queue
            .roundtrip(&mut self.state)
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(())
    }

    pub fn dispatch_pending(&mut self) -> Result<()> {
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(())
    }

    pub fn sync_roundtrip(&mut self) -> Result<()> {
        self.event_queue
            .roundtrip(&mut self.state)
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(())
    }

    pub fn get_state(&self) -> &InputMethodV1Data {
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

    pub fn event_queue(&mut self) -> &mut EventQueue<InputMethodV1Data> {
        &mut self.event_queue
    }

    pub fn get_context(&self) -> Option<ZwpInputMethodContextV1> {
        self.state.context.lock().ok().and_then(|ctx| ctx.clone())
    }

    pub fn commit_string(&self, text: &str) {
        if let Some(ctx) = self.get_context() {
            ctx.commit_string(self.state.serial, text.to_string());
        }
    }

    pub fn set_preedit(&self, text: &str, cursor: i32) {
        if let Some(ctx) = self.get_context() {
            ctx.preedit_cursor(cursor);
            ctx.preedit_string(self.state.serial, text.to_string(), "".to_string());
        }
    }

    pub fn clear_preedit(&self) {
        if let Some(ctx) = self.get_context() {
            ctx.preedit_string(self.state.serial, "".to_string(), "".to_string());
        }
    }

    pub fn flush(&self) -> Result<()> {
        self.connection
            .flush()
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(())
    }

    pub fn forward_key(&self, serial: u32, time: u32, key: u32, pressed: bool) {
        if let Some(ctx) = self.get_context() {
            let key_state: u32 = if pressed { 1 } else { 0 };
            ctx.key(serial, time, key, key_state);
            eprintln!(
                "DEBUG: Forwarded key: serial={}, key={}, pressed={}",
                serial, key, pressed
            );
        }
    }

    pub fn create_candidate_surface(&mut self) -> Result<()> {
        let compositor = self.compositor.as_ref().ok_or(Error::NoCompositor)?;
        let input_panel = self.input_panel.as_ref().ok_or(Error::NoInputPanel)?;

        let qh = self.event_queue.handle();

        let surface = compositor.create_surface(&qh, self.state.clone());

        let panel_surface = input_panel.get_input_panel_surface(&surface, &qh, self.state.clone());
        panel_surface.set_overlay_panel();

        self.candidate_surface = Some(surface);
        self.panel_surface = Some(panel_surface);

        self.connection
            .flush()
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        eprintln!("DEBUG: Created candidate panel surface with overlay_panel");
        Ok(())
    }

    pub fn show_candidate_window(
        &mut self,
        width: u32,
        height: u32,
        candidates: &[CandidateItem],
        highlighted_index: usize,
        primary_color: (u8, u8, u8),
    ) -> Result<()> {
        eprintln!("DEBUG: show_candidate_window called with width={}, height={}, candidates={}, highlighted={}", width, height, candidates.len(), highlighted_index);

        if self.candidate_surface.is_none() {
            self.create_candidate_surface()?;
        }

        if let Some(buffer) = self.current_buffer.take() {
            buffer.destroy();
        }
        if let Some(pool) = self.current_pool.take() {
            pool.destroy();
        }

        let shm = self.shm.as_ref().ok_or(Error::NoShm)?;
        let surface = self.candidate_surface.as_ref().unwrap();

        let qh = self.event_queue.handle();

        let stride = width * 4;
        let size = stride * height;
        eprintln!("DEBUG: stride={}, size={}", stride, size);

        let (pool, fd) = self.create_shm_pool_with_fd(shm, size)?;
        self.current_pool = Some(pool.clone());

        self.draw_candidates(
            &fd,
            width,
            height,
            candidates,
            highlighted_index,
            primary_color,
        )?;

        eprintln!(
            "DEBUG: About to create_buffer: offset=0, width={}, height={}, stride={}",
            width, height, stride
        );
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
        eprintln!("DEBUG: Buffer created successfully");

        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();

        eprintln!(
            "DEBUG: Showed candidate window {}x{} with {} candidates",
            width,
            height,
            candidates.len()
        );
        Ok(())
    }

    fn draw_candidates(
        &self,
        fd: &OwnedFd,
        width: u32,
        height: u32,
        candidates: &[CandidateItem],
        highlighted_index: usize,
        primary_color: (u8, u8, u8),
    ) -> Result<()> {
        let size = (width * height * 4) as usize;
        let size_nonzero = std::num::NonZero::new(size).expect("size should be non-zero");

        let ptr = unsafe {
            nix::sys::mman::mmap(
                None,
                size_nonzero,
                nix::sys::mman::ProtFlags::PROT_READ | nix::sys::mman::ProtFlags::PROT_WRITE,
                nix::sys::mman::MapFlags::MAP_SHARED,
                fd,
                0,
            )
            .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?
        };

        let pixels: &mut [u8] = unsafe { slice::from_raw_parts_mut(ptr.as_ptr() as *mut u8, size) };

        draw_candidates_to_buffer(
            pixels,
            width,
            height,
            candidates,
            highlighted_index,
            primary_color,
        );

        unsafe {
            nix::sys::mman::munmap(ptr, size)
                .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;
        }

        Ok(())
    }

    fn create_shm_pool_with_fd(&self, shm: &WlShm, size: u32) -> Result<(WlShmPool, OwnedFd)> {
        eprintln!("DEBUG: create_shm_pool_with_fd size={}", size);
        let qh = self.event_queue.handle();

        let fd = Self::create_anonymous_file(size)?;

        let pool = shm.create_pool(fd.as_fd(), size as i32, &qh, self.state.clone());
        Ok((pool, fd))
    }

    pub fn hide_candidate_window(&mut self) {
        if let Some(surface) = &self.candidate_surface {
            surface.attach(None::<&WlBuffer>, 0, 0);
            surface.commit();
            eprintln!("DEBUG: Hidden candidate window");
        }
    }

    /// Show a single key root display window
    /// Displays "a: 工匚戈艹廿龷七弋戈" in a small popup
    pub fn show_root_window(
        &mut self,
        key: char,
        root: &str,
        primary_color: (u8, u8, u8),
    ) -> Result<()> {
        eprintln!(
            "DEBUG: show_root_window called for key={}, root={}",
            key, root
        );

        let width = calculate_root_width(key, root, primary_color);
        let height = 36;

        // Use candidate_surface to display root (same surface, different content)
        let shm = self.shm.as_ref().ok_or(Error::NoShm)?;
        let surface = self
            .candidate_surface
            .as_ref()
            .ok_or(Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No candidate surface",
            )))?;

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

        let (pool, fd) = self.create_shm_pool_with_fd(shm, size)?;
        self.current_pool = Some(pool.clone());

        self.draw_root(&fd, width, height, key, root, primary_color)?;

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

        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        surface.commit();

        self.connection
            .flush()
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        eprintln!(
            "DEBUG: Showed root window {}x{} for key {} (on candidate surface)",
            width, height, key
        );
        Ok(())
    }

    pub fn hide_root_window(&mut self) {
        // No need to hide, main loop will restore candidate display
        eprintln!("DEBUG: hide_root_window called - will restore candidate on next update");
    }

    fn draw_root(
        &self,
        fd: &OwnedFd,
        width: u32,
        height: u32,
        key: char,
        root: &str,
        primary_color: (u8, u8, u8),
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

        draw_root_to_buffer(pixels, width, height, key, root, primary_color);

        unsafe {
            nix::sys::mman::munmap(ptr, size)
                .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e as i32)))?;
        }

        Ok(())
    }

    fn create_anonymous_file(size: u32) -> Result<OwnedFd> {
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
