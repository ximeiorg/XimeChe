mod config;

use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::Arc;
use std::os::unix::io::{OwnedFd, FromRawFd, AsRawFd};
use std::thread;
use std::io::Write;
use nix::unistd::dup;
use zbus::{Connection, interface};
use zbus::zvariant::Fd;
use xime_wayland::{WaylandConnectionV1, InputMethodV1State};
use xime_xkb::XkbContext;
use xime_tray::{TrayManager, InputMode, MenuAction};
use librime::traits::Traits;
use config::XimeConfig;
use xime_xkb::keysym_to_letter;

fn log_file() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    std::path::PathBuf::from(home).join(".config/xime/xime.log")
}

macro_rules! log_msg {
    ($($arg:tt)*) => {
        {
            let msg = format!("[{}] {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), format!($($arg)*));
            eprintln!("{}", msg.trim());
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_file())
            {
                let _ = f.write_all(msg.as_bytes());
            }
        }
    };
}

pub(crate) enum DaemonCommand {
    OpenWaylandSocket(OwnedFd, String),
    ToggleMode,
    Deploy,
    Shutdown,
}

pub struct XimeDaemon {
    command_tx: Sender<DaemonCommand>,
}

impl Clone for XimeDaemon {
    fn clone(&self) -> Self {
        Self {
            command_tx: self.command_tx.clone(),
        }
    }
}

impl XimeDaemon {
    fn new_with_channel(command_tx: Sender<DaemonCommand>) -> Self {
        Self { command_tx }
    }
}

#[interface(name = "org.xime.Xime.Controller")]
impl XimeDaemon {
    async fn open_wayland_socket(
        &self,
        fd: Fd<'_>,
        display: String,
    ) -> zbus::fdo::Result<()> {
        let raw_fd = fd.as_raw_fd();
        log_msg!("DEBUG: Received OpenWaylandSocket(fd={}, display={})", raw_fd, display);
        
        let dup_fd = dup(raw_fd)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to dup fd: {}", e)))?;
        let owned_fd = unsafe { OwnedFd::from_raw_fd(dup_fd) };
        
        self.command_tx
            .send(DaemonCommand::OpenWaylandSocket(owned_fd, display))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        
        Ok(())
    }
}

fn run_wayland_loop(rx: Receiver<DaemonCommand>, tray: Arc<TrayManager>, rt: tokio::runtime::Handle) {
    log_msg!("DEBUG: Wayland loop thread started");
    
    let mut conn: Option<WaylandConnectionV1> = None;
    let mut xkb: Option<XkbContext> = None;
    
    // Load Xime config for hotkeys and wubi roots
    let xime_config = XimeConfig::load();
    let _last_key_root_binding = xime_config.get_last_key_root_binding();
    log_msg!("DEBUG: Loaded hotkeys: show_last_key_root={}", 
              xime_config.hotkeys.show_last_key_root);
    
    let config_dir = get_config_dir();
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)
            .expect("Failed to create config directory");
        log_msg!("DEBUG: Created config directory: {}", config_dir.display());
    }
    let config_dir_str = config_dir.to_string_lossy().to_string();
    
    let mut traits = Traits::new();
    traits.set_shared_data_dir("/usr/share/rime-data");
    traits.set_user_data_dir(&config_dir_str);
    traits.set_log_dir(&config_dir_str);
    
    librime::setup(&mut traits);
    if let Err(e) = librime::initialize(&mut traits) {
        log_msg!("ERROR: Failed to initialize Rime: {}", e);
        return;
    }
    
    match librime::full_deploy_and_wait() {
        librime::DeployResult::Success => log_msg!("DEBUG: Rime deployed"),
        librime::DeployResult::Failure => log_msg!("WARNING: Rime deploy failed"),
    }
    
    if librime::is_maintenance_mode() {
        librime::join_maintenance_thread();
    }
    
    let mut rime_session = librime::create_session().ok();
    let mut candidate_window_visible = false;
    let mut last_input_keysym: Option<u32> = None;  // Record last input key
    let mut ctrl_root_visible = false;  // Is Ctrl showing root window?
    let mut last_ascii_mode = false;
    let mut last_state = InputMethodV1State::Inactive;
    
    loop {
        match rx.try_recv() {
            Ok(DaemonCommand::OpenWaylandSocket(fd, display)) => {
                log_msg!("DEBUG: Connecting from fd for display {}", display);
                
                xkb = XkbContext::new().ok();
                
                match WaylandConnectionV1::connect_from_fd(fd) {
                    Ok(c) => {
                        if c.get_input_method().is_ok() {
                            log_msg!("DEBUG: zwp_input_method_v1 available");
                        } else {
                            log_msg!("WARNING: zwp_input_method_v1 not available");
                        }
                        conn = Some(c);
                    }
                    Err(e) => {
                        log_msg!("ERROR: Failed to connect: {}", e);
                    }
                }
            }
            Ok(DaemonCommand::ToggleMode) => {
                log_msg!("DEBUG: ToggleMode command received");
                if let Some(session) = rime_session.as_ref() {
                    // Get current ascii_mode and toggle it directly
                    let current_ascii = session.get_option("ascii_mode").unwrap_or(false);
                    let new_ascii = !current_ascii;
                    session.set_option("ascii_mode", new_ascii).ok();
                    log_msg!("DEBUG: Set ascii_mode to {}", new_ascii);
                    
                    last_ascii_mode = new_ascii;
                    let tray_mode = if new_ascii {
                        InputMode::English
                    } else {
                        InputMode::Chinese
                    };
                    rt.block_on(async {
                        tray.set_mode(tray_mode).await;
                    });
                    log_msg!("DEBUG: Tray updated after toggle: ascii_mode={}", new_ascii);
                }
            }
            Ok(DaemonCommand::Deploy) => {
                log_msg!("DEBUG: Deploy command received, starting Rime deployment...");
                librime::finalize();
                
                let mut traits = Traits::new();
                traits.set_shared_data_dir("/usr/share/rime-data");
                traits.set_user_data_dir(&config_dir_str);
                traits.set_log_dir(&config_dir_str);
                
                librime::setup(&mut traits);
                if let Err(e) = librime::initialize(&mut traits) {
                    log_msg!("ERROR: Failed to reinitialize Rime: {}", e);
                } else {
                    match librime::full_deploy_and_wait() {
                        librime::DeployResult::Success => log_msg!("DEBUG: Rime redeployed successfully"),
                        librime::DeployResult::Failure => log_msg!("WARNING: Rime deploy failed"),
                    }
                    
                    if librime::is_maintenance_mode() {
                        librime::join_maintenance_thread();
                    }
                    
                    rime_session = librime::create_session().ok();
                    log_msg!("DEBUG: New Rime session created after deployment");
                }
            }
            Ok(DaemonCommand::Shutdown) => {
                log_msg!("DEBUG: Shutdown requested");
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                log_msg!("DEBUG: Command channel disconnected");
                break;
            }
        }
        
        if let Some(c) = conn.as_mut() {
            if let Err(e) = c.dispatch_events() {
                log_msg!("DEBUG: Dispatch error: {}", e);
                conn = None;
                rt.block_on(async {
                    tray.set_visible(false).await;
                });
                last_state = InputMethodV1State::Inactive;
                continue;
            }
            
            let state = c.get_state();
            
            // Handle state changes (activate/deactivate)
            if state.state != last_state {
                log_msg!("DEBUG: State changed from {:?} to {:?}", last_state, state.state);
                let is_active = state.state == InputMethodV1State::Active;
                rt.block_on(async {
                    tray.set_visible(is_active).await;
                });
                last_state = state.state;
                
                if !is_active {
                    candidate_window_visible = false;
                    continue;
                }
            }
            
            if state.state == InputMethodV1State::Active {
                if let Some(ref mut x) = xkb {
                    if let Some((fd, size)) = c.get_keymap_pending() {
                        if let Err(e) = x.set_keymap_from_owned_fd(fd, size) {
                            log_msg!("DEBUG: Keymap error: {}", e);
                        }
                    }
                    
                    let (depressed, latched, locked, group) = c.get_modifiers();
                    x.update_modifiers(depressed, latched, locked, group);
                }
                
                let events = c.pop_key_events();
                for event in events {
                    log_msg!("DEBUG: Key event: keycode={}, pressed={}", event.key, event.pressed);
                    
                    if let Some(ref mut x) = xkb {
                        let keysym = x.key_from_keycode(event.key + 8);
                        if let Some(sym) = keysym {
                            let modifiers = x.get_modifiers();
                            let release_mask = if !event.pressed { librime::K_RELEASE_MASK } else { 0 };
                            log_msg!("DEBUG: keysym={}, modifiers={}, release={}", sym.raw(), modifiers.effective, release_mask);
                            
                            // Handle Ctrl key for showing last input's root
                            // XK_Control_L = 0xFFE3 (65507), XK_Control_R = 0xFFE4 (65508)
                            let is_ctrl = sym.raw() == 0xFFE3 || sym.raw() == 0xFFE4;
                            log_msg!("DEBUG: is_ctrl={}, candidate_visible={}, last_key={:?}", is_ctrl, candidate_window_visible, last_input_keysym);
                            if candidate_window_visible && is_ctrl {
                                if event.pressed {
                                    let ctrl_pressed = modifiers.ctrl;
                                    let alt_pressed = modifiers.alt;
                                    let shift_pressed = modifiers.shift;
                                    let super_pressed = modifiers.super_key;
                                    
                                    // Only Ctrl pressed (no other modifiers)
                                    if ctrl_pressed && !alt_pressed && !shift_pressed && !super_pressed {
                                        if let Some(last_key) = last_input_keysym {
                                            let letter = keysym_to_letter(last_key);
                                            log_msg!("DEBUG: last_key={}, letter={:?}", last_key, letter);
                                            if let Some(letter) = letter {
                                                let root = xime_config.get_root_for_key(letter);
                                                log_msg!("DEBUG: root for '{}' = {:?}", letter, root);
                                                if let Some(root) = root {
                                                    log_msg!("DEBUG: Ctrl pressed, showing root for '{}': {}", letter, root);
                                                    let primary_color = xime_config.get_primary_color();
                                                    if let Err(e) = c.show_root_window(letter, &root, primary_color) {
                                                        log_msg!("DEBUG: Failed to show root window: {}", e);
                                                    } else {
                                                        ctrl_root_visible = true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else if ctrl_root_visible {
                                    log_msg!("DEBUG: Ctrl released, restoring candidate window");
                                    c.hide_root_window();
                                    ctrl_root_visible = false;
                                    
                                    // Restore candidate window from current Rime state
                                    if let Some(session) = rime_session.as_ref() {
                                        if let Some(ctx) = session.context() {
                                            let menu = ctx.menu();
                                            if menu.num_candidates > 0 {
                                                let candidate_items: Vec<xime_ui::CandidateItem> = 
                                                    menu.candidates.iter().enumerate().map(|(i, x)| {
                                                        let comment = x.comment.map(|c| c.to_string()).unwrap_or_default();
                                                        xime_ui::CandidateItem {
                                                            text: x.text.to_string(),
                                                            comment,
                                                            index: i,
                                                        }
                                                    }).collect();
                                                let highlighted_index = menu.highlighted_candidate_index;
                                                let width = xime_ui::calculate_candidate_width(&candidate_items);
                                                let height = 36;
                                                if let Err(e) = c.show_candidate_window(width, height, &candidate_items, highlighted_index) {
                                                    log_msg!("DEBUG: Failed to restore candidate window: {}", e);
                                                }
                                                if let Err(e) = c.flush() {
                                                    log_msg!("DEBUG: Failed to flush: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                                continue;  // Don't pass Ctrl to Rime when candidate visible
                            }
                            
                            if let Some(session) = rime_session.as_ref() {
                                let result = session.process_key(
                                    sym.raw() as i32,
                                    modifiers.effective as i32 | release_mask as i32,
                                );
                                log_msg!("DEBUG: Rime result: {:?}", result);
                                
                                // Record last input key when Rime processed it successfully
                                if result && event.pressed {
                                    let letter = keysym_to_letter(sym.raw());
                                    if letter.is_some() {
                                        last_input_keysym = Some(sym.raw());
                                        log_msg!("DEBUG: Recorded last input keysym={}", sym.raw());
                                    }
                                }
                                
                                if let Ok(status) = session.status() {
                                    let is_ascii = status.is_ascii_mode;
                                    if is_ascii != last_ascii_mode {
                                        last_ascii_mode = is_ascii;
                                        let tray_mode = if is_ascii {
                                            InputMode::English
                                        } else {
                                            InputMode::Chinese
                                        };
                                        rt.block_on(async {
                                            tray.set_mode(tray_mode).await;
                                        });
                                        log_msg!("DEBUG: Tray updated: ascii_mode={}", is_ascii);
                                    }
                                    log_msg!("DEBUG: ascii_mode={}, composing={}", is_ascii, status.is_composing);
                                }
                                
                                if !result {
                                    c.forward_key(event.serial, event.time, event.key, event.pressed);
                                }
                                
                                if let Some(commit) = session.commit() {
                                    c.commit_string(commit.text());
                                    let _ = c.flush();
                                    log_msg!("DEBUG: Committed: {}", commit.text());
                                }
                                
                                if let Some(ctx) = session.context() {
                                    if let Some(p) = ctx.composition().preedit {
                                        c.set_preedit(p, p.len() as i32);
                                    } else {
                                        c.clear_preedit();
                                    }
                                    let _ = c.flush();
                                    
                                    let menu = ctx.menu();
                                    if menu.num_candidates > 0 {
                                        let candidate_items: Vec<xime_ui::CandidateItem> = 
                                            menu.candidates.iter().enumerate().map(|(i, x)| {
                                                let comment = x.comment.map(|c| c.to_string()).unwrap_or_default();
                                                log_msg!("DEBUG: candidate {} text='{}' comment='{}'", i, x.text, comment);
                                                xime_ui::CandidateItem {
                                                    text: x.text.to_string(),
                                                    comment,
                                                    index: i,
                                                }
                                            }).collect();
                                        let highlighted_index = menu.highlighted_candidate_index;
                                        log_msg!("DEBUG: highlighted_index={}", highlighted_index);
                                        let width = xime_ui::calculate_candidate_width(&candidate_items);
                                        let height = 36;
                                        if let Err(e) = c.show_candidate_window(width, height, &candidate_items, highlighted_index) {
                                            log_msg!("DEBUG: Candidate window error: {}", e);
                                        }
                                        candidate_window_visible = true;
                                    } else if candidate_window_visible {
                                        c.hide_candidate_window();
                                        let _ = c.flush();
                                        candidate_window_visible = false;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        thread::sleep(std::time::Duration::from_millis(1));
    }
    
    librime::finalize();
}

fn get_config_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    std::path::PathBuf::from(home).join(".config/xime/rime")
}

fn main() -> anyhow::Result<()> {
    log_msg!("DEBUG: xime-daemon starting");
    
    let rt = tokio::runtime::Runtime::new()?;
    let rt_handle = rt.handle().clone();
    
    rt.block_on(async {
        let connection = Connection::session().await?;
        
        let (tray, mut toggle_rx, mut action_rx) = TrayManager::register(&connection).await?;
        let tray = Arc::new(tray);
        
        let (command_tx, command_rx) = mpsc::channel();
        
        thread::spawn({
            let tray = tray.clone();
            let rt_handle = rt_handle.clone();
            move || {
                run_wayland_loop(command_rx, tray, rt_handle);
            }
        });
        
        let daemon = XimeDaemon::new_with_channel(command_tx.clone());
        
        connection.object_server()
            .at("/org/xime/Xime", daemon)
            .await?;
        
        connection.request_name("org.xime.Xime").await?;
        
        log_msg!("DEBUG: DBus service registered at org.xime.Xime");
        log_msg!("DEBUG: Tray icon registered");
        log_msg!("DEBUG: Waiting for Wayland connection from launcher...");
        
        loop {
            tokio::select! {
                Some(_) = toggle_rx.recv() => {
                    log_msg!("DEBUG: Toggle request received from tray");
                    command_tx.send(DaemonCommand::ToggleMode).ok();
                }
                Some(action) = action_rx.recv() => {
                    log_msg!("DEBUG: Menu action received: {:?}", action);
                    match action {
                        MenuAction::ToggleMode => {
                            command_tx.send(DaemonCommand::ToggleMode).ok();
                        }
                        MenuAction::Settings => {
                            let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                            let setup_path = format!("{}/.local/bin/xime-setup", home);
                            std::process::Command::new(&setup_path)
                                .spawn()
                                .map_err(|e| log_msg!("ERROR: Failed to launch xime-setup: {}", e))
                                .ok();
                        }
                        MenuAction::Deploy => {
                            command_tx.send(DaemonCommand::Deploy).ok();
                        }
                        MenuAction::Exit => {
                            command_tx.send(DaemonCommand::Shutdown).ok();
                            break;
                        }
                    }
                }
            }
        }
        
        Ok::<(), anyhow::Error>(())
    })?;
    
    Ok(())
}