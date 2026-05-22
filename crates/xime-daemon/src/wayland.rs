use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;

use tracing::{debug, error, info, warn};
use xime_config::XimeConfig;
use xime_predict::{check_model_exists, Predictor};
use xime_tray::{InputMode, TrayManager};
use xime_wayland::{InputMethodV1State, WaylandConnectionV1};
use xime_xkb::XkbContext;
use xime_xkb::{keysym_to_letter, Keysym, ModifierState};

use crate::{DaemonCommand, RimeEngine};

pub struct WaylandLoop {
    command_rx: Receiver<DaemonCommand>,
    tray: Arc<TrayManager>,
    rt_handle: tokio::runtime::Handle,
}

impl WaylandLoop {
    pub fn new(
        command_rx: Receiver<DaemonCommand>,
        tray: Arc<TrayManager>,
        rt_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            command_rx,
            tray,
            rt_handle,
        }
    }

    pub fn run(self) {
        info!("Wayland loop thread started");

        let mut conn: Option<WaylandConnectionV1> = None;
        let mut xkb: Option<XkbContext> = None;
        let mut rime = RimeEngine::new();

        let mut predictor = if check_model_exists(None) {
            Predictor::new(None).ok()
        } else {
            None
        };
        if predictor.is_some() {
            info!("Smart suggestion model loaded successfully");
        } else {
            debug!("Smart suggestion model not available");
        }

        let mut xime_config = XimeConfig::load();
        let _last_key_root_binding = xime_config.get_last_key_root_binding();
        let primary_color = xime_config.get_primary_color();
        self.rt_handle.block_on(async {
            self.tray.set_primary_color(primary_color).await;
        });
        debug!(
            "Loaded hotkeys: show_key={}, primary_color={:?}",
            xime_config.wubi_radicals.hotkeys.show_key, primary_color
        );

        let mut candidate_window_visible = false;
        let mut last_input_keysym: Option<u32> = None;
        let mut ctrl_root_visible = false;
        let mut last_ascii_mode = false;
        let mut last_state = InputMethodV1State::Inactive;
        let mut smart_suggestion_visible = false;
        let mut last_committed_text = String::new();

        loop {
            use std::sync::mpsc::TryRecvError;

            match self.command_rx.try_recv() {
                Ok(DaemonCommand::OpenWaylandSocket(fd, display_name)) => {
                    debug!("Connecting from fd for display {}", display_name);

                    xkb = XkbContext::new().ok();

                    match WaylandConnectionV1::connect_from_fd(fd) {
                        Ok(c) => {
                            if c.get_input_method().is_ok() {
                                debug!("zwp_input_method_v1 available");
                            } else {
                                warn!("zwp_input_method_v1 not available");
                            }
                            conn = Some(c);
                        }
                        Err(e) => {
                            error!("Failed to connect: {}", e);
                        }
                    }
                }
                Ok(DaemonCommand::ToggleMode) => {
                    debug!("ToggleMode command received");
                    if let Some(_) = rime.session() {
                        let new_ascii = rime.toggle_ascii_mode();
                        last_ascii_mode = new_ascii;
                        let tray_mode = if new_ascii {
                            InputMode::English
                        } else {
                            InputMode::Chinese
                        };
                        self.rt_handle.block_on(async {
                            self.tray.set_mode(tray_mode).await;
                        });
                        debug!("Tray updated after toggle: ascii_mode={}", new_ascii);
                    }
                }
                Ok(DaemonCommand::Deploy) => {
                    debug!("Deploy command received, starting Rime deployment...");
                    rime.redeploy();
                }
                Ok(DaemonCommand::ReloadStyle) => {
                    debug!("ReloadStyle command received, reloading xime config...");
                    let new_config = XimeConfig::load();
                    let new_color = new_config.get_primary_color();
                    self.rt_handle.block_on(async {
                        self.tray.set_primary_color(new_color).await;
                    });
                    xime_config = new_config;
                    debug!("Style config reloaded, new primary_color={:?}", new_color);
                }
                Ok(DaemonCommand::Shutdown) => {
                    debug!("Shutdown requested");
                    break;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    debug!("Command channel disconnected");
                    break;
                }
            }

            if let Some(c) = conn.as_mut() {
                if let Err(e) = c.dispatch_events() {
                    debug!("Dispatch error: {}", e);
                    conn = None;
                    self.rt_handle.block_on(async {
                        self.tray.set_visible(false).await;
                    });
                    last_state = InputMethodV1State::Inactive;
                    continue;
                }

                let state = c.get_state();

                if state.state != last_state {
                    debug!("State changed from {:?} to {:?}", last_state, state.state);
                    let is_active = state.state == InputMethodV1State::Active;
                    self.rt_handle.block_on(async {
                        self.tray.set_visible(is_active).await;
                    });
                    last_state = state.state;

                    if !is_active {
                        candidate_window_visible = false;
                        smart_suggestion_visible = false;
                        last_committed_text.clear();
                        continue;
                    }
                }

                if state.state == InputMethodV1State::Active {
                    self.handle_active_state(
                        c,
                        &mut xkb,
                        &mut rime,
                        &xime_config,
                        &mut candidate_window_visible,
                        &mut last_input_keysym,
                        &mut ctrl_root_visible,
                        &mut last_ascii_mode,
                        &mut smart_suggestion_visible,
                        &mut predictor,
                        &mut last_committed_text,
                    );
                }
            }

            thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_active_state(
        &self,
        c: &mut WaylandConnectionV1,
        xkb: &mut Option<XkbContext>,
        rime: &mut RimeEngine,
        xime_config: &XimeConfig,
        candidate_window_visible: &mut bool,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        last_ascii_mode: &mut bool,
        smart_suggestion_visible: &mut bool,
        predictor: &mut Option<Predictor>,
        last_committed_text: &mut String,
    ) {
        if let Some(ref mut x) = xkb {
            if let Some((fd, size)) = c.get_keymap_pending() {
                if let Err(e) = x.set_keymap_from_owned_fd(fd, size) {
                    debug!("Keymap error: {}", e);
                }
            }

            let (depressed, latched, locked, group) = c.get_modifiers();
            x.update_modifiers(depressed, latched, locked, group);
        }

        let events = c.pop_key_events();
        for event in events {
            debug!(
                "Key event: keycode={}, pressed={}",
                event.key, event.pressed
            );

            if let Some(ref mut x) = xkb {
                let keysym = x.key_from_keycode(event.key + 8);
                if let Some(sym) = keysym {
                    self.handle_key_event(
                        c,
                        x,
                        rime,
                        xime_config,
                        event,
                        sym,
                        candidate_window_visible,
                        last_input_keysym,
                        ctrl_root_visible,
                        last_ascii_mode,
                        smart_suggestion_visible,
                        predictor,
                        last_committed_text,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_key_event(
        &self,
        c: &mut WaylandConnectionV1,
        xkb: &XkbContext,
        rime: &mut RimeEngine,
        xime_config: &XimeConfig,
        event: xime_wayland::KeyEvent,
        sym: Keysym,
        candidate_window_visible: &mut bool,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        last_ascii_mode: &mut bool,
        smart_suggestion_visible: &mut bool,
        predictor: &mut Option<Predictor>,
        last_committed_text: &mut String,
    ) {
        let modifiers = xkb.get_modifiers();
        let release_mask = if !event.pressed {
            librime::K_RELEASE_MASK
        } else {
            0
        };
        debug!(
            "keysym={}, modifiers={}, release={}",
            sym.raw(),
            modifiers.effective,
            release_mask
        );

        let is_ctrl = sym.raw() == 0xFFE3 || sym.raw() == 0xFFE4;
        debug!(
            "is_ctrl={}, candidate_visible={}, last_key={:?}",
            is_ctrl, candidate_window_visible, last_input_keysym
        );

        if *candidate_window_visible && is_ctrl {
            if self.handle_ctrl_key(
                c,
                xime_config,
                &event,
                sym,
                modifiers,
                candidate_window_visible,
                last_input_keysym,
                ctrl_root_visible,
                rime,
            ) {
                return;
            }
        }

        if let Some(session) = rime.session() {
            let result = session.process_key(
                sym.raw() as i32,
                modifiers.effective as i32 | release_mask as i32,
            );
            debug!("Rime result: {:?}", result);

            if result && event.pressed {
                let letter = keysym_to_letter(sym.raw());
                if letter.is_some() {
                    *last_input_keysym = Some(sym.raw());
                    debug!("Recorded last input keysym={}", sym.raw());
                }
            }

            if let Ok(status) = session.status() {
                let is_ascii = status.is_ascii_mode;
                if is_ascii != *last_ascii_mode {
                    *last_ascii_mode = is_ascii;
                    let tray_mode = if is_ascii {
                        InputMode::English
                    } else {
                        InputMode::Chinese
                    };
                    self.rt_handle.block_on(async {
                        self.tray.set_mode(tray_mode).await;
                    });
                    debug!("Tray updated: ascii_mode={}", is_ascii);
                }
                debug!("ascii_mode={}, composing={}", is_ascii, status.is_composing);
            }

            if !result {
                c.forward_key(event.serial, event.time, event.key, event.pressed);
            }

            if let Some(commit) = session.commit() {
                let committed = commit.text();
                c.commit_string(committed);
                let _ = c.flush();
                debug!("Committed: {}", committed);

                *last_committed_text = committed.to_string();

                if xime_config.smart_suggestion.enabled && !*last_ascii_mode {
                    self.show_smart_suggestions(
                        c,
                        xime_config,
                        predictor,
                        smart_suggestion_visible,
                        last_committed_text,
                    );
                }
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
                    let candidate_items: Vec<xime_ui::CandidateItem> = menu
                        .candidates
                        .iter()
                        .enumerate()
                        .map(|(i, x)| {
                            let comment = x.comment.map(|c| c.to_string()).unwrap_or_default();
                            debug!("candidate {} text='{}' comment='{}'", i, x.text, comment);
                            xime_ui::CandidateItem {
                                text: x.text.to_string(),
                                comment,
                                index: i,
                            }
                        })
                        .collect();
                    let highlighted_index = menu.highlighted_candidate_index;
                    debug!("highlighted_index={}", highlighted_index);
                    let width = xime_ui::calculate_candidate_width(&candidate_items);
                    let height = 36;
                    let primary_color = xime_config.get_primary_color();
                    if let Err(e) = c.show_candidate_window(
                        width,
                        height,
                        &candidate_items,
                        highlighted_index,
                        primary_color,
                    ) {
                        debug!("Candidate window error: {}", e);
                    }
                    *candidate_window_visible = true;
                    *smart_suggestion_visible = false;
                } else if *candidate_window_visible {
                    c.hide_candidate_window();
                    let _ = c.flush();
                    *candidate_window_visible = false;
                }
            }
        }
    }

    fn show_smart_suggestions(
        &self,
        c: &mut WaylandConnectionV1,
        xime_config: &XimeConfig,
        predictor: &mut Option<Predictor>,
        smart_suggestion_visible: &mut bool,
        last_committed_text: &str,
    ) {
        if predictor.is_none() || last_committed_text.is_empty() {
            return;
        }

        let prefix = if last_committed_text.len() > 4 {
            last_committed_text
                .chars()
                .rev()
                .take(4)
                .collect::<String>()
        } else {
            last_committed_text.to_string()
        };

        if let Some(ref mut p) = predictor {
            let suggestions = p.predict(
                &prefix,
                xime_config.smart_suggestion.suggestion_count as usize,
            );
            if let Ok(suggestions) = suggestions {
                if !suggestions.is_empty() {
                    debug!("Smart suggestions for '{}': {:?}", prefix, suggestions);

                    let candidate_items: Vec<xime_ui::CandidateItem> = suggestions
                        .iter()
                        .enumerate()
                        .map(|(i, (text, _score))| xime_ui::CandidateItem {
                            text: text.clone(),
                            comment: String::new(),
                            index: i,
                        })
                        .collect();

                    let width = xime_ui::calculate_candidate_width(&candidate_items);
                    let height = 36;
                    let primary_color = xime_config.get_primary_color();

                    if let Err(e) =
                        c.show_candidate_window(width, height, &candidate_items, 0, primary_color)
                    {
                        debug!("Smart suggestion window error: {}", e);
                    } else {
                        *smart_suggestion_visible = true;
                        if let Err(e) = c.flush() {
                            debug!("Flush error: {}", e);
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_ctrl_key(
        &self,
        c: &mut WaylandConnectionV1,
        xime_config: &XimeConfig,
        event: &xime_wayland::KeyEvent,
        _sym: Keysym,
        modifiers: ModifierState,
        _candidate_window_visible: &mut bool,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        rime: &mut RimeEngine,
    ) -> bool {
        if event.pressed {
            let ctrl_pressed = modifiers.ctrl;
            let alt_pressed = modifiers.alt;
            let shift_pressed = modifiers.shift;
            let super_pressed = modifiers.super_key;

            if ctrl_pressed && !alt_pressed && !shift_pressed && !super_pressed {
                if let Some(last_key) = *last_input_keysym {
                    let letter = keysym_to_letter(last_key);
                    debug!("last_key={}, letter={:?}", last_key, letter);
                    if let Some(letter) = letter {
                        let schema = rime.get_current_schema().unwrap_or_default();
                        let root = xime_config.get_root_for_key(&schema, letter);
                        debug!("root for '{}' (schema={}) = {:?}", letter, schema, root);
                        if let Some(root) = root {
                            debug!("Ctrl pressed, showing root for '{}': {}", letter, root);
                            let primary_color = xime_config.get_primary_color();
                            if let Err(e) = c.show_root_window(letter, &root, primary_color) {
                                debug!("Failed to show root window: {}", e);
                            } else {
                                *ctrl_root_visible = true;
                            }
                        }
                    }
                }
            }
        } else if *ctrl_root_visible {
            debug!("Ctrl released, restoring candidate window");
            c.hide_root_window();
            *ctrl_root_visible = false;

            if let Some(session) = rime.session() {
                if let Some(ctx) = session.context() {
                    let menu = ctx.menu();
                    if menu.num_candidates > 0 {
                        let candidate_items: Vec<xime_ui::CandidateItem> = menu
                            .candidates
                            .iter()
                            .enumerate()
                            .map(|(i, x)| {
                                let comment = x.comment.map(|c| c.to_string()).unwrap_or_default();
                                xime_ui::CandidateItem {
                                    text: x.text.to_string(),
                                    comment,
                                    index: i,
                                }
                            })
                            .collect();
                        let highlighted_index = menu.highlighted_candidate_index;
                        let width = xime_ui::calculate_candidate_width(&candidate_items);
                        let height = 36;
                        let primary_color = xime_config.get_primary_color();
                        if let Err(e) = c.show_candidate_window(
                            width,
                            height,
                            &candidate_items,
                            highlighted_index,
                            primary_color,
                        ) {
                            debug!("Failed to restore candidate window: {}", e);
                        }
                        if let Err(e) = c.flush() {
                            debug!("Failed to flush: {}", e);
                        }
                    }
                }
            }
        }
        true
    }
}
