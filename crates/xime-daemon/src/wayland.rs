use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;

use tracing::{debug, error, info};
use xime_config::XimeConfig;
use xime_plugin::EmojiItem;
use xime_tray::{InputMode, TrayManager};
use xime_wayland::{connect_im_from_fd, connect_im_to_env, ImBackend};
use xime_xkb::XkbContext;
use xime_xkb::{keysym_to_letter, Keysym, ModifierState};

use crate::{DaemonCommand, PluginHost, RimeEngine};

/// emoji 面板状态：`;` 触发，字符搜索，数字选择上屏。
///
/// 候选列表第 1 位固定为分号 `;`（数字 1 输入分号），随后是表情。
struct EmojiPanel {
    active: bool,
    query: String,
    items: Vec<EmojiItem>,
    highlighted: usize,
    /// 候选总数（含分号位），与 Rime 候选页大小一致。
    page_size: usize,
}

impl Default for EmojiPanel {
    fn default() -> Self {
        Self {
            active: false,
            query: String::new(),
            items: Vec::new(),
            highlighted: 0,
            page_size: 5,
        }
    }
}

/// 数字键 → 候选索引（1-9 对应 0-8，0 对应 9）。
fn emoji_select_index(keysym: u32) -> Option<usize> {
    match keysym {
        0x31..=0x39 => Some((keysym - 0x31) as usize),
        0x30 => Some(9),
        _ => None,
    }
}

/// 候选索引 → 实际提交文本（0 = 分号，其余 = 表情 items 下标-1）。
fn emoji_commit_text(panel: &EmojiPanel, index: usize) -> Option<String> {
    if index == 0 {
        Some(";".to_string())
    } else {
        panel.items.get(index - 1).map(|e| e.text.clone())
    }
}

/// 决定按键是否转发给应用（孤儿释放抑制）。
/// - Rime 消费的按下（`press_consumed`）不转发；其释放事件也不转发，避免孤儿释放。
/// - 其他按键（未被消费的按下、未被消费的释放）正常转发。
fn should_forward_key(
    pressed: bool,
    result: bool,
    consumed_presses: &std::collections::HashSet<u32>,
    key: u32,
) -> bool {
    let press_consumed = result && pressed;
    let release_of_consumed = !pressed && consumed_presses.contains(&key);
    !press_consumed && !release_of_consumed
}

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

        let mut conn: Option<Box<dyn ImBackend>> = None;
        let mut xkb: Option<XkbContext> = None;
        let mut rime = RimeEngine::new();
        let mut plugin_host = PluginHost::new();
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
        let mut emoji_panel = EmojiPanel::default();
        let mut consumed_presses: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut last_input_keysym: Option<u32> = None;
        let mut ctrl_root_visible = false;
        let mut last_ascii_mode = false;
        let mut last_active = false;

        // 无 launcher 的会话（GNOME 等）：直接连接 $WAYLAND_DISPLAY 使用 v2 协议。
        // KWin 下普通 socket 不暴露 IM 协议，此步会失败，随后等待 launcher 传入 fd。
        match connect_im_to_env() {
            Ok(backend) => {
                info!("Connected directly to compositor (standalone mode)");
                conn = Some(backend);
            }
            Err(e) => {
                debug!(
                    "Direct connection not available (waiting for launcher fd): {}",
                    e
                );
            }
        }

        loop {
            use std::sync::mpsc::TryRecvError;

            match self.command_rx.try_recv() {
                Ok(DaemonCommand::OpenWaylandSocket(fd, display_name)) => {
                    debug!("Connecting from fd for display {}", display_name);

                    xkb = XkbContext::new().ok();

                    match connect_im_from_fd(fd) {
                        Ok(backend) => {
                            info!("Connected via launcher fd (KWin mode)");
                            conn = Some(backend);
                        }
                        Err(e) => {
                            error!("Failed to connect: {}", e);
                        }
                    }
                }
                Ok(DaemonCommand::ToggleMode) => {
                    debug!("ToggleMode command received");
                    if rime.session().is_some() {
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
                Ok(DaemonCommand::ReloadPlugins) => {
                    debug!("ReloadPlugins command received, reloading plugins...");
                    plugin_host.reload();
                }
                Ok(DaemonCommand::SelectSchema(schema_id, result_tx)) => {
                    debug!("SelectSchema command received: {}", schema_id);
                    let ok = rime.select_schema(&schema_id);
                    let _ = result_tx.send(ok);
                    debug!("SelectSchema result: {}", ok);
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
                    last_active = false;
                    continue;
                }

                if let Err(e) = c.handle_unavailable() {
                    debug!("handle_unavailable error: {}", e);
                }

                let is_active = c.is_active();

                if is_active != last_active {
                    debug!("State changed: active={}", is_active);
                    self.rt_handle.block_on(async {
                        self.tray.set_visible(is_active).await;
                    });
                    last_active = is_active;

                    if !is_active {
                        candidate_window_visible = false;
                        continue;
                    }
                }

                if is_active {
                    self.handle_active_state(
                        c.as_mut(),
                        &mut xkb,
                        &mut rime,
                        &mut plugin_host,
                        &mut emoji_panel,
                        &xime_config,
                        &mut candidate_window_visible,
                        &mut consumed_presses,
                        &mut last_input_keysym,
                        &mut ctrl_root_visible,
                        &mut last_ascii_mode,
                    );
                }
            }

            thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_active_state(
        &self,
        c: &mut dyn ImBackend,
        xkb: &mut Option<XkbContext>,
        rime: &mut RimeEngine,
        plugin_host: &mut PluginHost,
        emoji_panel: &mut EmojiPanel,
        xime_config: &XimeConfig,
        candidate_window_visible: &mut bool,
        consumed_presses: &mut std::collections::HashSet<u32>,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        last_ascii_mode: &mut bool,
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
                        plugin_host,
                        emoji_panel,
                        xime_config,
                        event,
                        sym,
                        candidate_window_visible,
                        consumed_presses,
                        last_input_keysym,
                        ctrl_root_visible,
                        last_ascii_mode,
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_key_event(
        &self,
        c: &mut dyn ImBackend,
        xkb: &XkbContext,
        rime: &mut RimeEngine,
        plugin_host: &mut PluginHost,
        emoji_panel: &mut EmojiPanel,
        xime_config: &XimeConfig,
        event: xime_wayland::KeyEvent,
        sym: Keysym,
        candidate_window_visible: &mut bool,
        consumed_presses: &mut std::collections::HashSet<u32>,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        last_ascii_mode: &mut bool,
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

        // emoji 面板：`;` 触发，面板激活时按键全部由面板消费。
        if self.handle_emoji_key(
            c,
            plugin_host,
            emoji_panel,
            &event,
            sym,
            xime_config,
            candidate_window_visible,
        ) {
            return;
        }

        if *candidate_window_visible
            && is_ctrl
            && self.handle_ctrl_key(
                c,
                xime_config,
                &event,
                sym,
                modifiers,
                candidate_window_visible,
                last_input_keysym,
                ctrl_root_visible,
                rime,
            )
        {
            return;
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

            // 回车键（Return / 数字键盘回车）：与其他按键一致，按 Rime 消费结果决定转发。
            // - 组合态下 Rime 吞掉回车并提交编码（result=true），不再转发，
            //   应用只收到上屏的编码文本，不会多出回车符。
            // - 空组合态下 Rime 不消费（result=false），转发给应用（正常换行/执行命令）。
            // 被吞按下的释放事件同样抑制（孤儿释放抑制，对齐 fcitx5）。
            if should_forward_key(event.pressed, result, consumed_presses, event.key) {
                c.forward_key(event.serial, event.time, event.key, event.pressed);
            }
            if result && event.pressed {
                consumed_presses.insert(event.key);
            } else {
                consumed_presses.remove(&event.key);
            }

            if let Some(commit) = session.commit() {
                let committed = commit.text();
                c.commit_string(committed);
                let _ = c.flush();
                debug!("Committed: {}", committed);
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
                    let primary_color = xime_config.get_primary_color();
                    if let Err(e) =
                        c.show_candidate_window(&candidate_items, highlighted_index, primary_color)
                    {
                        debug!("Candidate window error: {}", e);
                    }
                    *candidate_window_visible = true;
                } else if *candidate_window_visible {
                    c.hide_candidate_window();
                    let _ = c.flush();
                    *candidate_window_visible = false;
                }
            }
        }
    }

    /// emoji 面板按键处理。返回 true 表示按键已被面板消费。
    ///
    /// - `;`（中文态，无组合输入）：进入面板，候选第 1 位为分号本身
    /// - 面板激活时：
    ///   - 可打印字符：追加搜索词并实时刷新
    ///   - BackSpace：删除搜索词末尾
    ///   - 数字键：1 输入分号，其余选择对应表情并上屏
    ///   - Return/Space：选择高亮候选并上屏
    ///   - Escape：退出面板
    #[allow(clippy::too_many_arguments)]
    fn handle_emoji_key(
        &self,
        c: &mut dyn ImBackend,
        plugin_host: &mut PluginHost,
        panel: &mut EmojiPanel,
        event: &xime_wayland::KeyEvent,
        sym: Keysym,
        xime_config: &XimeConfig,
        candidate_window_visible: &mut bool,
    ) -> bool {
        if !event.pressed {
            return panel.active;
        }

        let raw = sym.raw();

        if !panel.active {
            // 只有中文态分号触发，避免影响英文输入
            if raw == ';' as u32 {
                // 兜底：触发前重载插件，确保 daemon 早于插件安装启动时也能用。
                plugin_host.reload();
                if plugin_host.emoji_plugin_count() == 0 {
                    debug!("Emoji trigger but no emoji plugins loaded");
                    return false;
                }
                panel.active = true;
                panel.query.clear();
                panel.page_size = xime_config.style.candidate_count.max(2) as usize;
                self.refresh_emoji_panel(c, plugin_host, panel, candidate_window_visible);
                debug!("Emoji panel activated (page_size={})", panel.page_size);
            }
            return panel.active;
        }

        // ---- 面板激活：全部按键由面板消费 ----
        match raw {
            0xFF1B => {
                // Escape：退出
                panel.active = false;
                self.hide_emoji_panel(c, candidate_window_visible);
                debug!("Emoji panel exited via Escape");
            }
            0xFF08 => {
                // BackSpace：删除搜索词
                panel.query.pop();
                self.refresh_emoji_panel(c, plugin_host, panel, candidate_window_visible);
            }
            0xFF0D | 0xFF8D | 0x20 => {
                // Return / KP_Enter / Space：选中高亮
                if let Some(text) = emoji_commit_text(panel, panel.highlighted) {
                    c.commit_string(&text);
                    let _ = c.flush();
                    debug!("Emoji committed: {}", text);
                }
                panel.active = false;
                self.hide_emoji_panel(c, candidate_window_visible);
            }
            0xFF09 | 0xFF53 | 0x2015 => {
                // Tab / Right / 无：切换高亮到下一个（有限支持）
                if panel.highlighted + 1 < panel.items.len() + 1 {
                    panel.highlighted = (panel.highlighted + 1) % (panel.items.len() + 1);
                    self.show_emoji_candidates(c, panel, candidate_window_visible);
                }
            }
            k if emoji_select_index(k).is_some() => {
                // 数字键选择候选（1-9 对应索引 0-8，0 对应 9）
                let index = emoji_select_index(k).unwrap_or(0);
                if let Some(text) = emoji_commit_text(panel, index) {
                    c.commit_string(&text);
                    let _ = c.flush();
                    debug!("Emoji committed by key {}: {}", k, text);
                }
                panel.active = false;
                self.hide_emoji_panel(c, candidate_window_visible);
            }
            k if (0x20..0x7F).contains(&k) => {
                // 其他可打印 ASCII 字符：追加搜索
                let ch = char::from_u32(k).unwrap_or(' ');
                panel.query.push(ch);
                panel.highlighted = 0;
                self.refresh_emoji_panel(c, plugin_host, panel, candidate_window_visible);
            }
            _ => {
                // 其他键（修饰键等）：忽略，不退出面板
                return true;
            }
        }
        true
    }

    fn refresh_emoji_panel(
        &self,
        c: &mut dyn ImBackend,
        plugin_host: &PluginHost,
        panel: &mut EmojiPanel,
        candidate_window_visible: &mut bool,
    ) {
        // 第 1 位固定为分号，表情取剩余 page_size-1 个
        panel.items = plugin_host.query_emojis(&panel.query, panel.page_size - 1);
        if panel.highlighted > panel.items.len() {
            panel.highlighted = 0;
        }
        debug!(
            "Emoji search '{}': {} results (page_size={})",
            panel.query,
            panel.items.len(),
            panel.page_size
        );
        if panel.items.is_empty() {
            self.hide_emoji_panel(c, candidate_window_visible);
        } else {
            self.show_emoji_candidates(c, panel, candidate_window_visible);
        }
    }

    fn show_emoji_candidates(
        &self,
        c: &mut dyn ImBackend,
        panel: &EmojiPanel,
        candidate_window_visible: &mut bool,
    ) {
        // 第 1 位：分号本身（数字 1 输入分号）
        let mut candidates = vec![xime_ui::CandidateItem {
            text: ";".to_string(),
            comment: "分号".to_string(),
            index: 0,
        }];
        candidates.extend(panel.items.iter().enumerate().map(|(i, e)| {
            xime_ui::CandidateItem {
                text: e.text.clone(),
                comment: e.category.clone(),
                index: i + 1,
            }
        }));
        if let Err(e) = c.show_candidate_window(&candidates, panel.highlighted, (0x8F, 0x73, 0xE2))
        {
            debug!("Emoji candidate window error: {}", e);
        }
        *candidate_window_visible = true;
    }

    fn hide_emoji_panel(&self, c: &mut dyn ImBackend, candidate_window_visible: &mut bool) {
        c.hide_candidate_window();
        let _ = c.flush();
        *candidate_window_visible = false;
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_ctrl_key(
        &self,
        c: &mut dyn ImBackend,
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
                        let primary_color = xime_config.get_primary_color();
                        if let Err(e) = c.show_candidate_window(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_should_forward_key_empty() {
        let consumed = HashSet::new();
        // 空组合态按下：未消费 → 转发
        assert!(should_forward_key(true, false, &consumed, 10));
        // 空组合态释放：未消费 → 转发
        assert!(should_forward_key(false, false, &consumed, 10));
    }

    #[test]
    fn test_should_forward_key_consumed_press() {
        let mut consumed = HashSet::new();
        // Rime 消费的按下（如组合态回车）：不转发
        assert!(!should_forward_key(true, true, &consumed, 10));
        consumed.insert(10);
        // 其释放事件：不转发（孤儿释放抑制）
        assert!(!should_forward_key(false, false, &consumed, 10));
    }

    #[test]
    fn test_should_forward_key_after_commit() {
        let consumed = HashSet::new();
        // 组合态回车提交编码后，同键的第二次按下（已清空组合）：正常转发
        assert!(should_forward_key(true, false, &consumed, 10));
    }

    #[test]
    fn test_should_forward_key_different_keys() {
        let mut consumed = HashSet::new();
        consumed.insert(10);
        // 10 的释放被抑制，但其他键不受影响
        assert!(!should_forward_key(false, false, &consumed, 10));
        assert!(should_forward_key(false, false, &consumed, 20));
    }

    #[test]
    fn test_emoji_select_index() {
        // 1-9 → 0-8
        for (key, idx) in [(0x31, 0usize), (0x35, 4), (0x39, 8)] {
            assert_eq!(emoji_select_index(key), Some(idx));
        }
        // 0 → 9
        assert_eq!(emoji_select_index(0x30), Some(9));
        // 非数字键不映射
        assert_eq!(emoji_select_index(0x20), None);
        assert_eq!(emoji_select_index(0x61), None);
        assert_eq!(emoji_select_index(0xFF0D), None);
    }

    #[test]
    fn test_emoji_commit_text() {
        // 空面板：索引 0 是分号，其余无
        let panel = EmojiPanel::default();
        assert_eq!(emoji_commit_text(&panel, 0).as_deref(), Some(";"));
        assert_eq!(emoji_commit_text(&panel, 1), None);

        // 有表情：索引 0 分号，索引 1.. 表情
        let panel = EmojiPanel {
            items: vec![
                EmojiItem {
                    id: "k1".into(),
                    text: "(ﾟ∀ﾟ)".into(),
                    image_url: None,
                    category: "颜文字".into(),
                },
                EmojiItem {
                    id: "k2".into(),
                    text: "(^u^)".into(),
                    image_url: None,
                    category: "颜文字".into(),
                },
            ],
            ..EmojiPanel::default()
        };
        assert_eq!(emoji_commit_text(&panel, 0).as_deref(), Some(";"));
        assert_eq!(emoji_commit_text(&panel, 1).as_deref(), Some("(ﾟ∀ﾟ)"));
        assert_eq!(emoji_commit_text(&panel, 2).as_deref(), Some("(^u^)"));
        assert_eq!(emoji_commit_text(&panel, 3), None);
    }
}
