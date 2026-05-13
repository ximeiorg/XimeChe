use std::sync::mpsc::{self, Sender, Receiver};
use std::os::unix::io::{OwnedFd, FromRawFd, AsRawFd};
use std::thread;
use nix::unistd::dup;
use zbus::{Connection, interface};
use zbus::zvariant::Fd;
use xime_wayland::{WaylandConnectionV1, InputMethodV1State};
use xime_xkb::XkbContext;
use librime::{traits::Traits, session::Session, K_RELEASE_MASK};

enum DaemonCommand {
    OpenWaylandSocket(OwnedFd, String),
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
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        
        thread::spawn(|| {
            run_wayland_loop(rx);
        });
        
        Self { command_tx: tx }
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
        eprintln!("DEBUG: Received OpenWaylandSocket(fd={}, display={})", raw_fd, display);
        
        // Duplicate the fd because zbus::Fd will close it when dropped
        let dup_fd = dup(raw_fd)
            .map_err(|e| zbus::fdo::Error::Failed(format!("Failed to dup fd: {}", e)))?;
        let owned_fd = unsafe { OwnedFd::from_raw_fd(dup_fd) };
        
        self.command_tx
            .send(DaemonCommand::OpenWaylandSocket(owned_fd, display))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        
        Ok(())
    }
}

fn run_wayland_loop(rx: Receiver<DaemonCommand>) {
    eprintln!("DEBUG: Wayland loop thread started");
    
    let mut conn: Option<WaylandConnectionV1> = None;
    let mut xkb: Option<XkbContext> = None;
    
    let config_dir = get_config_dir();
    let config_dir_str = config_dir.to_string_lossy().to_string();
    
    let mut traits = Traits::new();
    traits.set_shared_data_dir("/usr/share/rime-data");
    traits.set_user_data_dir(&config_dir_str);
    traits.set_log_dir(&config_dir_str);
    
    librime::setup(&mut traits);
    if let Err(e) = librime::initialize(&mut traits) {
        eprintln!("ERROR: Failed to initialize Rime: {}", e);
        return;
    }
    
    match librime::full_deploy_and_wait() {
        librime::DeployResult::Success => eprintln!("DEBUG: Rime deployed"),
        librime::DeployResult::Failure => eprintln!("WARNING: Rime deploy failed"),
    }
    
    if librime::is_maintenance_mode() {
        librime::join_maintenance_thread();
    }
    
    let rime_session = librime::create_session().ok();
    let mut candidate_window_visible = false;
    
    loop {
        match rx.try_recv() {
            Ok(DaemonCommand::OpenWaylandSocket(fd, display)) => {
                eprintln!("DEBUG: Connecting from fd for display {}", display);
                
                xkb = XkbContext::new().ok();
                
                match WaylandConnectionV1::connect_from_fd(fd) {
                    Ok(c) => {
                        if c.get_input_method().is_ok() {
                            eprintln!("DEBUG: zwp_input_method_v1 available");
                        } else {
                            eprintln!("WARNING: zwp_input_method_v1 not available");
                        }
                        conn = Some(c);
                    }
                    Err(e) => {
                        eprintln!("ERROR: Failed to connect: {}", e);
                    }
                }
            }
            Ok(DaemonCommand::Shutdown) => {
                eprintln!("DEBUG: Shutdown requested");
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                eprintln!("DEBUG: Command channel disconnected");
                break;
            }
        }
        
        if let Some(c) = conn.as_mut() {
            if let Err(e) = c.dispatch_events() {
                eprintln!("DEBUG: Dispatch error: {}", e);
                conn = None;
                continue;
            }
            
            let state = c.get_state();
            
            if state.state == InputMethodV1State::Active {
                if let Some(ref mut x) = xkb {
                    if let Some((fd, size)) = c.get_keymap_pending() {
                        if let Err(e) = x.set_keymap_from_owned_fd(fd, size) {
                            eprintln!("DEBUG: Keymap error: {}", e);
                        }
                    }
                    
                    let (depressed, latched, locked, group) = c.get_modifiers();
                    x.update_modifiers(depressed, latched, locked, group);
                }
                
                let events = c.pop_key_events();
                for event in events {
                    eprintln!("DEBUG: Key event: keycode={}, pressed={}", event.key, event.pressed);
                    
                    if let Some(ref mut x) = xkb {
                        let keysym = x.key_from_keycode(event.key + 8);
                        if let Some(sym) = keysym {
                            let modifiers = x.get_modifiers();
                            let release_mask = if !event.pressed { librime::K_RELEASE_MASK } else { 0 };
                            eprintln!("DEBUG: keysym={}, modifiers={}, release={}", sym.raw(), modifiers.effective, release_mask);
                            
                            if let Some(session) = rime_session.as_ref() {
                                let result = session.process_key(
                                    sym.raw() as i32,
                                    modifiers.effective as i32 | release_mask as i32,
                                );
                                eprintln!("DEBUG: Rime result: {:?}", result);
                                
                                if let Ok(status) = session.status() {
                                    eprintln!("DEBUG: ascii_mode={}, composing={}", status.is_ascii_mode, status.is_composing);
                                }
                                
                                if !result {
                                    c.forward_key(event.serial, event.time, event.key, event.pressed);
                                }
                                
if let Some(commit) = session.commit() {
                                        c.commit_string(commit.text());
                                        let _ = c.flush();
                                        eprintln!("DEBUG: Committed: {}", commit.text());
                                    }
                                
if let Some(ctx) = session.context() {
                                        // Handle preedit FIRST (input encoding), regardless of candidates
                                        if let Some(p) = ctx.composition().preedit {
                                            c.set_preedit(p, p.len() as i32);
                                        } else {
                                            c.clear_preedit();
                                        }
                                        let _ = c.flush();
                                        
                                        // Then handle candidate window
                                        let menu = ctx.menu();
                                        if menu.num_candidates > 0 {
                                            let candidate_texts: Vec<String> = 
                                                menu.candidates.iter().map(|x| x.text.to_string()).collect();
                                            eprintln!("DEBUG: Candidates: {:?}", candidate_texts);
                                            
                                            let width = xime_ui::calculate_candidate_width(&candidate_texts);
                                            let height = 36;
                                            if let Err(e) = c.show_candidate_window(width, height, &candidate_texts) {
                                                eprintln!("DEBUG: Candidate window error: {}", e);
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
    eprintln!("DEBUG: xime-daemon starting");
    
    let rt = tokio::runtime::Runtime::new()?;
    
    rt.block_on(async {
        let daemon = XimeDaemon::new();
        
        let connection = Connection::session().await?;
        
        connection.object_server()
            .at("/org/xime/Xime", daemon)
            .await?;
        
        connection.request_name("org.xime.Xime").await?;
        
        eprintln!("DEBUG: DBus service registered at org.xime.Xime");
        eprintln!("DEBUG: Waiting for Wayland connection from launcher...");
        
        std::future::pending::<()>().await;
        
        Ok::<(), anyhow::Error>(())
    })?;
    
    Ok(())
}