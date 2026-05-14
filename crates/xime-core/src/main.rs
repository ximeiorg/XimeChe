use librime::{traits::Traits, session::Session};
use xime_wayland::{
    WaylandConnection, WaylandConnectionV1, 
    InputMethodState, InputMethodV1State,
    ErrorV2, ErrorV1,
};
use xime_xkb::XkbContext;
use xime_ui::CandidateList;
use std::env;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Wayland v2 error: {0}")]
    WaylandV2(#[from] ErrorV2),
    
    #[error("Wayland v1 error: {0}")]
    WaylandV1(#[from] ErrorV1),
    
    #[error("Rime error: {0}")]
    Rime(#[from] librime::error::Error),
    
    #[error("XKB error: {0}")]
    Xkb(#[from] xime_xkb::Error),
    
    #[error("UI error: {0}")]
    Ui(#[from] xime_ui::Error),
    
    #[error("Config error: {0}")]
    Config(String),
    
    #[error("No supported input method protocol available (tried v1 and v2)")]
    NoProtocolAvailable,
}

enum Backend {
    V2(WaylandConnection),
    V1(WaylandConnectionV1),
}

pub struct Xime {
    backend: Backend,
    rime_session: Session,
    #[allow(dead_code)]
    xkb: XkbContext,
    candidates: CandidateList,
}

impl Xime {
    pub fn new() -> Result<Self> {
        let backend = Self::connect_wayland()?;
        
        let config_dir = Self::get_config_dir();
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir)
                .map_err(|e| Error::Config(e.to_string()))?;
        }

        let config_dir_str = config_dir.to_string_lossy().to_string();
        let shared_data_dir = Self::get_shared_data_dir();
        
        let mut traits = Traits::new();
        traits.set_shared_data_dir(&shared_data_dir);
        traits.set_user_data_dir(&config_dir_str);
        traits.set_log_dir(&config_dir_str);
        
        librime::setup(&mut traits);
        librime::initialize(&mut traits)?;
        
        // Deploy schemas
        eprintln!("DEBUG: Deploying Rime schemas...");
        match librime::full_deploy_and_wait() {
            librime::DeployResult::Success => eprintln!("DEBUG: Deploy success"),
            librime::DeployResult::Failure => eprintln!("DEBUG: Deploy failed"),
        }
        
        if librime::is_maintenance_mode() {
            librime::join_maintenance_thread();
        }
        
        let rime_session = librime::create_session()?;
        let xkb = XkbContext::new()?;
        let candidates = CandidateList::new(5);
        
        Ok(Self {
            backend,
            rime_session,
            xkb,
            candidates,
        })
    }
    
    fn connect_wayland() -> Result<Backend> {
        // Try v1 first - for KWin
        // WAYLAND_SOCKET is consumed on first connect_to_env, so v1 must be first for KWin
        eprintln!("DEBUG: Trying input-method-v1 protocol (for KWin/Weston)...");
        match WaylandConnectionV1::connect() {
            Ok(conn) if conn.get_input_method().is_ok() => {
                eprintln!("DEBUG: input-method-v1 available, using v1 backend");
                return Ok(Backend::V1(conn));
            }
            Ok(conn) => {
                let has_v1_global = conn.has_zwp_input_method_v1_global();
                eprintln!("DEBUG: v1: connected, zwp_input_method_v1 global={}, bind result={}", has_v1_global, conn.get_input_method().is_ok());
                if has_v1_global && !conn.get_input_method().is_ok() {
                    eprintln!("WARNING: zwp_input_method_v1 global exists but bind failed!");
                    eprintln!("This usually means the compositor requires specific permissions or timing.");
                }
            }
            Err(e) => eprintln!("DEBUG: v1 connection error: {}", e),
        }
        
        // Try v2 - for Sway/Hyprland (WAYLAND_SOCKET may be consumed, will use WAYLAND_DISPLAY)
        eprintln!("DEBUG: Trying input-method-v2 protocol (for Sway/Hyprland/wlroots)...");
        match WaylandConnection::connect() {
            Ok(conn) if conn.get_input_method_manager().is_ok() => {
                eprintln!("DEBUG: input-method-v2 available, using v2 backend");
                return Ok(Backend::V2(conn));
            }
            Ok(_) => eprintln!("DEBUG: v2: connected but no input-method-v2 manager"),
            Err(e) => eprintln!("DEBUG: v2 connection error: {}", e),
        }
        
        Err(Error::NoProtocolAvailable)
    }

    fn get_config_dir() -> PathBuf {
        let home = env::var("HOME").unwrap_or_else(|_| "/".to_string());
        PathBuf::from(home).join(".config/xime/rime")
    }
    
    fn get_shared_data_dir() -> String {
        "/usr/share/rime-data".to_string()
    }

    pub fn run(&mut self) -> Result<()> {
        match &mut self.backend {
            Backend::V2(conn) => {
                conn.create_input_method()?;
                println!("Input method v2 created, running event loop...");
                
                let mut active = false;
                loop {
                    conn.dispatch_events()?;
                    
                    let state = conn.get_state();
                    if state.state == InputMethodState::Active && !active {
                        active = true;
                        println!("Input method activated");
                    } else if state.state == InputMethodState::Inactive && active {
                        active = false;
                        self.candidates.clear();
                        println!("Input method deactivated");
                    }
                    
                    if let Some(commit) = self.rime_session.commit() {
                        if let Some(im) = &conn.input_method {
                            im.commit_string(commit.text().to_string());
                            im.commit(state.serial);
                        }
                    }
                    
                    if let Some(ctx) = self.rime_session.context() {
                        let menu = ctx.menu();
                        if menu.num_candidates > 0 {
                            let c: Vec<(&str, Option<&str>)> = menu.candidates.iter().map(|x| (x.text, x.comment)).collect();
                            self.candidates.set_candidates(c, menu.select_keys.as_deref());
                            if let Some(p) = ctx.composition().preedit {
                                if let Some(im) = &conn.input_method {
                                    im.set_preedit_string(Some(p.to_string()), 0, p.len() as i32);
                                }
                            }
                        } else {
                            self.candidates.clear();
                            if let Some(im) = &conn.input_method {
                                im.set_preedit_string(None, 0, 0);
                            }
                        }
                        if let Some(im) = &conn.input_method {
                            im.commit(state.serial);
                        }
                    }
                }
            }
            Backend::V1(conn) => {
                eprintln!("DEBUG: Input method v1 running event loop...");
                
                let mut xkb = XkbContext::new()?;
                let mut active = false;
                let mut keymap_set = false;
                let mut candidate_window_visible = false;
                
                loop {
                    conn.dispatch_events()?;
                    
                    // Handle keymap if pending
                    if !keymap_set {
                        if let Some((fd, size)) = conn.get_keymap_pending() {
                            if let Err(e) = xkb.set_keymap_from_owned_fd(fd, size) {
                                eprintln!("DEBUG: Failed to set keymap: {}", e);
                            } else {
                                keymap_set = true;
                                eprintln!("DEBUG: Keymap set successfully");
                            }
                        }
                    }
                    
                    // Update modifiers
                    let (depressed, latched, locked, group) = conn.get_modifiers();
                    xkb.update_modifiers(depressed, latched, locked, group);
                    
                    // Process key events
                    let key_events = conn.pop_key_events();
                    for key_event in key_events {
                        if key_event.pressed {
                            eprintln!("DEBUG: Key pressed: keycode={}", key_event.key);
                            
                            // Convert keycode to keysym (keycode + 8 for XKB)
                            let keysym = xkb.key_from_keycode(key_event.key + 8);
                            
                            if let Some(sym) = keysym {
                                eprintln!("DEBUG: keysym={}", sym.raw());
                                
                                // Send to Rime
                                let modifiers = xkb.get_modifiers();
                                let result = self.rime_session.process_key(
                                    sym.raw() as i32,
                                    modifiers.effective as i32,
                                );
                                
                                eprintln!("DEBUG: Rime result: {:?}", result);
                            }
                        }
                    }
                    
                    // Small sleep to reduce CPU
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    
                    let state = conn.get_state();
                    if state.state == InputMethodV1State::Active && !active {
                        active = true;
                        eprintln!("DEBUG: Input method activated (v1)");
                    } else if state.state == InputMethodV1State::Inactive && active {
                        active = false;
                        self.candidates.clear();
                        conn.hide_candidate_window();
                        candidate_window_visible = false;
                        eprintln!("DEBUG: Input method deactivated (v1)");
                    }
                    
                    // Handle commit
                    if let Some(commit) = self.rime_session.commit() {
                        conn.commit_string(commit.text());
                        eprintln!("DEBUG: Committed: {}", commit.text());
                    }
                    
                    // Handle context (preedit and candidates)
                    if let Some(ctx) = self.rime_session.context() {
                        let menu = ctx.menu();
                        if menu.num_candidates > 0 {
                            let c: Vec<(&str, Option<&str>)> = menu.candidates.iter().map(|x| (x.text, x.comment)).collect();
                            self.candidates.set_candidates(c, menu.select_keys.as_deref());
                            
                            let candidate_items: Vec<xime_ui::CandidateItem> = menu.candidates.iter().enumerate().map(|(i, x)| {
                                let comment = x.comment.map(|c| c.to_string()).unwrap_or_default();
                                eprintln!("DEBUG: candidate {} text='{}' comment='{}'", i, x.text, comment);
                                xime_ui::CandidateItem {
                                    text: x.text.to_string(),
                                    comment,
                                    index: i,
                                }
                            }).collect();
                            
                            let highlighted_index = menu.highlighted_candidate_index;
                            let width = xime_ui::calculate_candidate_width(&candidate_items);
                            let height = 36;
                            if let Err(e) = conn.show_candidate_window(width, height, &candidate_items, highlighted_index) {
                                eprintln!("DEBUG: Failed to show candidate window: {}", e);
                            } else {
                                candidate_window_visible = true;
                            }
                            
                            if let Some(p) = ctx.composition().preedit {
                                conn.set_preedit(p, p.len() as i32);
                                eprintln!("DEBUG: Preedit: {}", p);
                            }
                        } else {
                            self.candidates.clear();
                            conn.clear_preedit();
                            if candidate_window_visible {
                                conn.hide_candidate_window();
                                candidate_window_visible = false;
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Xime {
    fn drop(&mut self) {
        librime::finalize();
    }
}

fn main() {
    // Debug: capture WAYLAND_SOCKET at startup
    if let Ok(socket) = env::var("WAYLAND_SOCKET") {
        eprintln!("DEBUG: WAYLAND_SOCKET={} at startup", socket);
    }
    if let Ok(display) = env::var("WAYLAND_DISPLAY") {
        eprintln!("DEBUG: WAYLAND_DISPLAY={} at startup", display);
    }
    
    println!("xime-wayland: Wayland input method with Rime engine");
    println!("Supports: input-method-v2 (Sway/Hyprland) and input-method-v1 (KWin/Weston)");
    
    match Xime::new() {
        Ok(mut xime) => {
            if let Err(e) = xime.run() {
                eprintln!("Error: {}", e);
            }
        }
        Err(e) => {
            eprintln!("Failed to initialize: {}", e);
            eprintln!("\nNote: KWin requires configuring xime in System Settings > Virtual Keyboard");
            // Exit immediately on error - don't keep process running
            std::process::exit(1);
        }
    }
}