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

use crate::{symbols, DaemonCommand, PluginHost, RimeEngine};

/// 搜索面板模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelMode {
    /// 表情：`;` 触发（或菜单「表情」），插件数据源。
    Emoji,
    /// 符号：菜单「符号」触发，内置符号表数据源。
    Symbols,
}

/// 搜索面板状态（表情/符号共用）：字符搜索，网格展示，点击/数字选择上屏。
///
/// 内容直接铺在菜单面板的网格中（不占候选栏），每页容量为网格容量。
/// 方向键语义与 Rime 候选一致：Left/Right 移动高亮，Up/Down 翻页。
struct SearchPanel {
    active: bool,
    mode: PanelMode,
    query: String,
    items: Vec<EmojiItem>,
    highlighted: usize,
    /// 当前页（0 起）。
    page: usize,
    /// 内容网格列数（按最宽项自适应，刷新时更新）。
    columns: usize,
}

/// 候选栏右侧面板路由状态。
enum PanelState {
    Closed,
    /// 菜单网格打开。
    MenuOpen,
    /// 内容网格打开（表情/符号）。
    ContentOpen,
}

impl Default for SearchPanel {
    fn default() -> Self {
        Self {
            active: false,
            mode: PanelMode::Emoji,
            query: String::new(),
            items: Vec::new(),
            highlighted: 0,
            page: 0,
            columns: xime_ui::content_columns_for(xime_ui::CONTENT_ITEM_SIZE),
        }
    }
}

impl SearchPanel {
    /// 每页容量（当前网格列数 × 行数）。
    fn per_page(&self) -> usize {
        xime_ui::content_capacity(self.columns)
    }

    /// 内容面板渲染宽度（与后端公式一致：按最宽项定单元格与列数）。
    fn panel_width(&self) -> u32 {
        let widest = self
            .items
            .iter()
            .map(|e| xime_ui::content_text_width(&e.text))
            .max()
            .unwrap_or(0);
        let cell = xime_ui::content_cell_width(widest);
        xime_ui::content_panel_width(cell, xime_ui::content_columns_for(cell))
    }

    /// 当前页项数（最后一页可能不满）。
    fn page_len(&self) -> usize {
        let start = self.page * self.per_page();
        self.items.len().saturating_sub(start).min(self.per_page())
    }

    fn total_pages(&self) -> usize {
        self.items.len().div_ceil(self.per_page())
    }

    /// 以指定模式重新打开面板（清空搜索词与高亮）。
    fn open(&mut self, mode: PanelMode) {
        self.mode = mode;
        self.active = true;
        self.query.clear();
        self.highlighted = 0;
        self.page = 0;
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

/// 页内索引 → 实际提交文本（无保留位，索引 0 即当前页第一个）。
fn panel_commit_text(panel: &SearchPanel, index: usize) -> Option<String> {
    let offset = panel.page * panel.per_page();
    panel.items.get(offset + index).map(|e| e.text.clone())
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

/// 最近一次候选窗内容（菜单开/关后重绘用）。
type CandidateCache = (Vec<xime_ui::CandidateItem>, usize, (u8, u8, u8));

pub struct WaylandLoop {
    command_rx: Receiver<DaemonCommand>,
    tray: Arc<TrayManager>,
    rt_handle: tokio::runtime::Handle,
    /// 最近一次候选内容缓存（菜单开/关后重绘用）。
    candidate_cache: std::sync::Mutex<Option<CandidateCache>>,
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
            candidate_cache: std::sync::Mutex::new(None),
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
        let mut search_panel = SearchPanel::default();
        let mut panel_state = PanelState::Closed;
        let mut last_panel_width: u32 = 0;
        let mut consumed_presses: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut last_input_keysym: Option<u32> = None;
        let mut ctrl_root_visible = false;
        let mut last_ascii_mode = false;
        let mut last_active = false;
        // 输入法启停开关（Ctrl+Space 切换）：停用态按键直接转发，不做任何处理。
        let mut im_enabled = true;

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
                        // 失焦（切换窗口/输入框）时彻底清理 UI 状态：
                        // 立即隐藏候选栏/菜单面板/Ctrl 字根窗口，关闭 emoji 面板，
                        // 清除按键消费记录与字根缓存。
                        // 否则残留的候选栏会一直显示在新输入框上，遮挡并吞掉
                        // 点击事件，导致新输入框无法获得焦点、输入法无法重新激活。
                        c.hide_candidate_window();
                        c.hide_menu_panel();
                        c.hide_root_window();
                        let _ = c.flush();
                        candidate_window_visible = false;
                        search_panel.active = false;
                        panel_state = PanelState::Closed;
                        ctrl_root_visible = false;
                        last_input_keysym = None;
                        consumed_presses.clear();
                        continue;
                    }
                }

                if is_active {
                    self.handle_active_state(
                        c.as_mut(),
                        &mut xkb,
                        &mut rime,
                        &mut plugin_host,
                        &mut search_panel,
                        &mut panel_state,
                        &mut last_panel_width,
                        &xime_config,
                        &mut candidate_window_visible,
                        &mut consumed_presses,
                        &mut last_input_keysym,
                        &mut ctrl_root_visible,
                        &mut last_ascii_mode,
                        &mut im_enabled,
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
        search_panel: &mut SearchPanel,
        panel_state: &mut PanelState,
        last_panel_width: &mut u32,
        xime_config: &XimeConfig,
        candidate_window_visible: &mut bool,
        consumed_presses: &mut std::collections::HashSet<u32>,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        last_ascii_mode: &mut bool,
        im_enabled: &mut bool,
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

        // 指针事件：菜单按钮 / 面板入口点击
        let pointer_events = c.pop_pointer_events();
        for pe in pointer_events {
            if pe.button != 272 || !pe.pressed {
                continue; // 只处理左键按下
            }
            debug!(
                "Pointer press: x={}, y={}, on_menu={}",
                pe.x, pe.y, pe.on_menu
            );
            self.handle_pointer_press(
                c,
                plugin_host,
                search_panel,
                panel_state,
                last_panel_width,
                xime_config,
                candidate_window_visible,
                &pe,
            );
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
                        search_panel,
                        panel_state,
                        last_panel_width,
                        xime_config,
                        event,
                        sym,
                        candidate_window_visible,
                        consumed_presses,
                        last_input_keysym,
                        ctrl_root_visible,
                        last_ascii_mode,
                        im_enabled,
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
        search_panel: &mut SearchPanel,
        panel_state: &mut PanelState,
        last_panel_width: &mut u32,
        xime_config: &XimeConfig,
        event: xime_wayland::KeyEvent,
        sym: Keysym,
        candidate_window_visible: &mut bool,
        consumed_presses: &mut std::collections::HashSet<u32>,
        last_input_keysym: &mut Option<u32>,
        ctrl_root_visible: &mut bool,
        last_ascii_mode: &mut bool,
        im_enabled: &mut bool,
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

        // Ctrl+Space：启停输入法（fcitx 风格）。任意状态下优先处理。
        if event.pressed && modifiers.ctrl && sym.raw() == 0x20 {
            *im_enabled = !*im_enabled;
            if *im_enabled {
                debug!("Input method enabled (Ctrl+Space)");
            } else {
                debug!("Input method disabled (Ctrl+Space)");
                // 停用：丢弃组合、清空 preedit、关闭全部 UI
                rime.clear_composition();
                c.clear_preedit();
                c.hide_candidate_window();
                c.hide_menu_panel();
                c.hide_root_window();
                let _ = c.flush();
                *candidate_window_visible = false;
                search_panel.active = false;
                *panel_state = PanelState::Closed;
                *ctrl_root_visible = false;
                *last_input_keysym = None;
                consumed_presses.clear();
                self.rt_handle.block_on(async {
                    self.tray.set_mode(InputMode::English).await;
                });
            }
            // 消费按下（释放由 consumed_presses 抑制，避免孤儿释放）
            consumed_presses.insert(event.key);
            return;
        }

        // 停用态：按键直接转发，不做任何处理（被消费按下的释放仍抑制）。
        if !*im_enabled {
            if event.pressed || !consumed_presses.contains(&event.key) {
                c.forward_key(event.serial, event.time, event.key, event.pressed);
            } else {
                consumed_presses.remove(&event.key);
            }
            return;
        }

        let is_ctrl = sym.raw() == 0xFFE3 || sym.raw() == 0xFFE4;
        debug!(
            "is_ctrl={}, candidate_visible={}, last_key={:?}",
            is_ctrl, candidate_window_visible, last_input_keysym
        );

        // 菜单面板打开时：任意按键（除修饰键）关闭面板并消费该键。
        if event.pressed
            && matches!(panel_state, PanelState::MenuOpen)
            && !is_ctrl
            && !matches!(sym.raw(), 0xFFE1 | 0xFFE2 | 0xFFE9 | 0xFFEA)
        {
            debug!("Key pressed while menu open, closing panel");
            *panel_state = PanelState::Closed;
            c.hide_menu_panel();
            self.redraw_menu_candidates(c, xime_config, candidate_window_visible);
            // 不转发：面板关闭后由后续按键处理正常输入
        }

        // emoji 面板：`;` 触发，面板激活时按键全部由面板消费。
        if self.handle_search_key(
            c,
            plugin_host,
            search_panel,
            panel_state,
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
                    *last_panel_width = c.candidate_width(&candidate_items);
                    // 缓存最近候选（菜单开/关后重绘）
                    if let Ok(mut cache) = self.candidate_cache.lock() {
                        *cache = Some((candidate_items.clone(), highlighted_index, primary_color));
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

    /// 搜索面板按键处理。返回 true 表示按键已被面板消费。
    ///
    /// - `;`（中文态，无组合输入）：进入表情面板（网格直接铺在面板区）
    /// - 面板激活时：
    ///   - 可打印字符：追加搜索词并实时刷新
    ///   - BackSpace：删除搜索词末尾
    ///   - 数字键：选择对应项并上屏（面板保持打开）
    ///   - Tab/Right/Left：移动高亮
    ///   - Return/Space：选择高亮项并上屏
    ///   - `;`：直接上屏分号
    ///   - Up/Down：翻页
    ///   - Escape：退出面板
    #[allow(clippy::too_many_arguments)]
    fn handle_search_key(
        &self,
        c: &mut dyn ImBackend,
        plugin_host: &mut PluginHost,
        panel: &mut SearchPanel,
        panel_state: &mut PanelState,
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
            // 只有中文态分号触发表情面板，避免影响英文输入
            if raw == ';' as u32 {
                // 兜底：触发前重载插件，确保 daemon 早于插件安装启动时也能用。
                plugin_host.reload();
                if plugin_host.emoji_plugin_count() == 0 {
                    debug!("Emoji trigger but no emoji plugins loaded");
                    return false;
                }
                panel.open(PanelMode::Emoji);
                *panel_state = PanelState::ContentOpen;
                self.refresh_search_panel(
                    c,
                    plugin_host,
                    panel,
                    panel_state,
                    xime_config,
                    candidate_window_visible,
                );
                debug!("Emoji panel activated");
            }
            return panel.active;
        }

        // ---- 面板激活：全部按键由面板消费 ----
        match raw {
            0xFF1B => {
                // Escape：退出面板，恢复候选栏
                panel.active = false;
                *panel_state = PanelState::Closed;
                c.hide_menu_panel();
                self.redraw_menu_candidates(c, xime_config, candidate_window_visible);
                debug!("Search panel exited via Escape");
            }
            0xFF08 => {
                // BackSpace：删除搜索词
                panel.query.pop();
                self.refresh_search_panel(
                    c,
                    plugin_host,
                    panel,
                    panel_state,
                    xime_config,
                    candidate_window_visible,
                );
            }
            0xFF0D | 0xFF8D | 0x20 => {
                // Return / KP_Enter / Space：选中高亮，面板保持打开
                if let Some(text) = panel_commit_text(panel, panel.highlighted) {
                    c.commit_string(&text);
                    let _ = c.flush();
                    debug!("Panel committed: {}", text);
                }
            }
            0x3B => {
                // 分号：直接上屏分号（面板保持打开）
                c.commit_string(";");
                let _ = c.flush();
                debug!("Semicolon committed from panel");
            }
            0xFF09 | 0xFF53 => {
                // Tab / Right：高亮移动到下一个（页内循环）
                let count = panel.page_len().max(1);
                panel.highlighted = (panel.highlighted + 1) % count;
                self.show_content(c, panel, xime_config, candidate_window_visible);
            }
            0xFF51 => {
                // Left：高亮移动到上一个
                let count = panel.page_len().max(1);
                panel.highlighted = (panel.highlighted + count - 1) % count;
                self.show_content(c, panel, xime_config, candidate_window_visible);
            }
            0xFF52 => {
                // Up：上一页
                if panel.page > 0 {
                    panel.page -= 1;
                    panel.highlighted = 0;
                    self.show_content(c, panel, xime_config, candidate_window_visible);
                }
            }
            0xFF54 => {
                // Down：下一页
                if panel.page + 1 < panel.total_pages() {
                    panel.page += 1;
                    panel.highlighted = 0;
                    self.show_content(c, panel, xime_config, candidate_window_visible);
                }
            }
            k if emoji_select_index(k).is_some() => {
                // 数字键选择（1-9 对应索引 0-8，0 对应 9），面板保持打开
                let index = emoji_select_index(k).unwrap_or(0);
                if let Some(text) = panel_commit_text(panel, index) {
                    c.commit_string(&text);
                    let _ = c.flush();
                    debug!("Panel committed by key {}: {}", k, text);
                }
            }
            k if (0x20..0x7F).contains(&k) => {
                // 其他可打印 ASCII 字符：追加搜索
                let ch = char::from_u32(k).unwrap_or(' ');
                panel.query.push(ch);
                panel.highlighted = 0;
                self.refresh_search_panel(
                    c,
                    plugin_host,
                    panel,
                    panel_state,
                    xime_config,
                    candidate_window_visible,
                );
            }
            _ => {
                // 其他键（修饰键等）：忽略，不退出面板
                return true;
            }
        }
        true
    }

    /// 面板内容刷新：查询数据源，渲染内容网格（表情/符号）。
    /// 结果为空时关闭面板。
    fn refresh_search_panel(
        &self,
        c: &mut dyn ImBackend,
        plugin_host: &PluginHost,
        panel: &mut SearchPanel,
        panel_state: &mut PanelState,
        xime_config: &XimeConfig,
        candidate_window_visible: &mut bool,
    ) {
        // 表情取多页（最多 3 页）；符号表完整保留（分页浏览）
        let limit = match panel.mode {
            PanelMode::Emoji => panel.per_page().saturating_mul(3),
            PanelMode::Symbols => usize::MAX,
        };
        panel.items = match panel.mode {
            PanelMode::Emoji => plugin_host.query_emojis(&panel.query, limit),
            PanelMode::Symbols => symbols::search(&panel.query, limit),
        };
        // 列数随最宽项自适应（影响每页容量与渲染宽度）
        let widest = panel
            .items
            .iter()
            .map(|e| xime_ui::content_text_width(&e.text))
            .max()
            .unwrap_or(0);
        let cell = xime_ui::content_cell_width(widest);
        panel.columns = xime_ui::content_columns_for(cell);
        panel.page = 0;
        panel.highlighted = 0;
        debug!(
            "Search panel '{}': {} results (mode={:?}, per_page={}, width={})",
            panel.query,
            panel.items.len(),
            panel.mode,
            panel.per_page(),
            panel.panel_width()
        );
        if panel.items.is_empty() {
            panel.active = false;
            *panel_state = PanelState::Closed;
            c.hide_menu_panel();
            self.redraw_menu_candidates(c, xime_config, candidate_window_visible);
        } else {
            self.show_content(c, panel, xime_config, candidate_window_visible);
        }
    }

    /// 渲染内容面板当前页网格（表情/符号直接铺在面板区，不占候选栏）。
    /// 面板状态设置后立即触发 show_candidate_window 渲染（复用最近候选，
    /// 无候选时以空候选栏渲染）。
    fn show_content(
        &self,
        c: &mut dyn ImBackend,
        panel: &SearchPanel,
        xime_config: &XimeConfig,
        candidate_window_visible: &mut bool,
    ) {
        // 当前页切片
        let per_page = panel.per_page();
        let start = panel.page * per_page;
        let end = (start + per_page).min(panel.items.len());
        let grid: Vec<xime_ui::GridItem> = panel.items[start..end]
            .iter()
            .map(|e| xime_ui::GridItem {
                text: e.text.clone(),
                comment: e.category.clone(),
            })
            .collect();
        if let Err(e) = c.show_content_panel(&grid, Some(panel.highlighted)) {
            debug!("Content panel error: {}", e);
        }
        // 渲染：优先复用最近候选，无则空候选栏
        let cached = self.candidate_cache.lock().ok().and_then(|g| g.clone());
        let (candidates, highlighted, primary_color) = match cached {
            Some((cands, hi, col)) => (cands, hi, col),
            None => (Vec::new(), 0usize, xime_config.get_primary_color()),
        };
        if let Err(e) = c.show_candidate_window(&candidates, highlighted, primary_color) {
            debug!("Content render candidate window error: {}", e);
        }
        let _ = c.flush();
        *candidate_window_visible = true;
    }

    /// 处理候选栏菜单按钮 / 面板入口点击。
    #[allow(clippy::too_many_arguments)]
    fn handle_pointer_press(
        &self,
        c: &mut dyn ImBackend,
        plugin_host: &mut PluginHost,
        search_panel: &mut SearchPanel,
        panel_state: &mut PanelState,
        last_panel_width: &u32,
        xime_config: &XimeConfig,
        candidate_window_visible: &mut bool,
        pe: &xime_wayland::PointerEvent,
    ) {
        match panel_state {
            PanelState::ContentOpen => {
                // 内容网格：点击项直接上屏（面板保持打开，可连续选择）
                let width = search_panel.panel_width().max(*last_panel_width);
                if let Some(idx) = xime_ui::content_item_hit(
                    pe.x,
                    pe.y,
                    width,
                    search_panel.columns,
                    search_panel.page_len(),
                ) {
                    if let Some(text) = panel_commit_text(search_panel, idx) {
                        c.commit_string(&text);
                        let _ = c.flush();
                        debug!("Content item committed: {}", text);
                    }
                    return;
                }
                // 菜单按钮：路由回菜单视图
                if xime_ui::menu_button_hit(pe.x, pe.y, width) {
                    debug!("Menu button clicked, routing back to menu");
                    let primary_color = xime_config.get_primary_color();
                    if let Err(e) = c.show_menu_panel(None, primary_color) {
                        debug!("Failed to show menu panel: {}", e);
                    } else {
                        *panel_state = PanelState::MenuOpen;
                        self.redraw_menu_candidates(c, xime_config, candidate_window_visible);
                    }
                }
                // 其他区域（候选栏空白）：忽略
            }
            PanelState::MenuOpen => {
                // 面板在候选栏下方展开：y >= 36 是面板区
                if pe.y >= xime_ui::CANDIDATE_HEIGHT as i32 {
                    // 点击面板入口
                    if let Some(action) = xime_ui::menu_item_hit(pe.x, pe.y, *last_panel_width) {
                        debug!("Menu item clicked: {:?}", action);
                        if !action.is_available() {
                            // 未实现的功能：保持菜单打开，不做任何事
                            debug!("Menu item {:?} not implemented, keeping menu open", action);
                            return;
                        }
                        *panel_state = PanelState::Closed;
                        c.hide_menu_panel();
                        match action {
                            xime_ui::MenuAction::Emoji => {
                                // 路由到表情网格（复用 `;` 触发的搜索面板）
                                plugin_host.reload();
                                if plugin_host.emoji_plugin_count() == 0 {
                                    debug!("Emoji menu click but no emoji plugins loaded");
                                    self.redraw_menu_candidates(
                                        c,
                                        xime_config,
                                        candidate_window_visible,
                                    );
                                    return;
                                }
                                search_panel.open(PanelMode::Emoji);
                            }
                            xime_ui::MenuAction::Symbols => {
                                // 路由到符号网格（内置符号表）
                                search_panel.open(PanelMode::Symbols);
                            }
                            _ => unreachable!("unavailable actions are filtered above"),
                        }
                        *panel_state = PanelState::ContentOpen;
                        self.refresh_search_panel(
                            c,
                            plugin_host,
                            search_panel,
                            panel_state,
                            xime_config,
                            candidate_window_visible,
                        );
                        debug!("Content panel opened from menu: {:?}", search_panel.mode);
                        return;
                    }
                    // 面板内但未命中入口：关闭
                    *panel_state = PanelState::Closed;
                    c.hide_menu_panel();
                    self.redraw_menu_candidates(c, xime_config, candidate_window_visible);
                } else {
                    // 点击候选栏区域
                    if xime_ui::menu_button_hit(pe.x, pe.y, *last_panel_width) {
                        *panel_state = PanelState::Closed;
                        c.hide_menu_panel();
                        self.redraw_menu_candidates(c, xime_config, candidate_window_visible);
                        debug!("Menu button clicked, closing panel");
                    }
                }
            }
            PanelState::Closed => {
                // 候选栏最右侧按钮区域
                if xime_ui::menu_button_hit(pe.x, pe.y, *last_panel_width) {
                    debug!("Menu button clicked, opening panel");
                    let primary_color = xime_config.get_primary_color();
                    if let Err(e) = c.show_menu_panel(None, primary_color) {
                        debug!("Failed to show menu panel: {}", e);
                    } else {
                        *panel_state = PanelState::MenuOpen;
                        self.redraw_menu_candidates(c, xime_config, candidate_window_visible);
                    }
                }
            }
        }
    }

    /// 菜单开/关后重绘候选栏（复用最近一次候选内容，实现面板增高/恢复效果）。
    /// 无缓存时隐藏候选窗（面板内容模式且此前无候选的情况）。
    fn redraw_menu_candidates(
        &self,
        c: &mut dyn ImBackend,
        xime_config: &XimeConfig,
        candidate_window_visible: &mut bool,
    ) {
        let cached = self.candidate_cache.lock().ok().and_then(|g| g.clone());
        if let Some((candidates, highlighted, _)) = cached {
            let primary_color = xime_config.get_primary_color();
            if let Err(e) = c.show_candidate_window(&candidates, highlighted, primary_color) {
                debug!("Menu redraw candidate window error: {}", e);
            }
            let _ = c.flush();
            *candidate_window_visible = true;
            debug!("Candidate window redrawn after menu state change");
        } else {
            c.hide_candidate_window();
            let _ = c.flush();
            *candidate_window_visible = false;
        }
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
    fn test_panel_commit_text() {
        // 空面板：无项
        let panel = SearchPanel::default();
        assert_eq!(panel_commit_text(&panel, 0), None);

        // 有项：索引 0 即第一项（无保留位）
        let panel = SearchPanel {
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
            ..SearchPanel::default()
        };
        assert_eq!(panel_commit_text(&panel, 0).as_deref(), Some("(ﾟ∀ﾟ)"));
        assert_eq!(panel_commit_text(&panel, 1).as_deref(), Some("(^u^)"));
        assert_eq!(panel_commit_text(&panel, 2), None);
        // 每页容量 = 内容网格容量
        assert_eq!(
            panel.per_page(),
            xime_ui::content_capacity(xime_ui::CONTENT_COLUMNS_MAX)
        );
    }

    #[test]
    fn test_panel_commit_text_paged() {
        // 每页容量 = 网格容量（30）；构造 35 项验证翻页偏移与页数
        let items: Vec<EmojiItem> = (0..35)
            .map(|i| EmojiItem {
                id: format!("k{i}"),
                text: format!("t{i}"),
                image_url: None,
                category: "c".into(),
            })
            .collect();
        let panel = SearchPanel {
            page: 1,
            items: items.clone(),
            ..SearchPanel::default()
        };
        // 第 2 页（offset=30）：索引 0=t30，索引 4=t34
        assert_eq!(panel_commit_text(&panel, 0).as_deref(), Some("t30"));
        assert_eq!(panel_commit_text(&panel, 4).as_deref(), Some("t34"));
        assert_eq!(panel_commit_text(&panel, 5), None);
        assert_eq!(panel.total_pages(), 2);
        assert_eq!(panel.page_len(), 5);
        // 首页满页
        let first = SearchPanel {
            items,
            ..SearchPanel::default()
        };
        assert_eq!(
            first.page_len(),
            xime_ui::content_capacity(xime_ui::CONTENT_COLUMNS_MAX)
        );
    }

    #[test]
    fn test_symbols_commit_text() {
        // 符号模式与表情模式提交语义一致（无保留位）
        let panel = SearchPanel {
            mode: PanelMode::Symbols,
            page: 1,
            items: (0..70)
                .map(|i| EmojiItem {
                    id: format!("s{i}"),
                    text: format!("s{i}"),
                    image_url: None,
                    category: "数".into(),
                })
                .collect(),
            ..SearchPanel::default()
        };
        assert_eq!(
            panel.per_page(),
            xime_ui::content_capacity(xime_ui::CONTENT_COLUMNS_MAX)
        );
        assert_eq!(panel.total_pages(), 3);
        // 第 2 页（offset=30）：索引 0=s30
        assert_eq!(panel_commit_text(&panel, 0).as_deref(), Some("s30"));
        assert_eq!(
            panel.page_len(),
            xime_ui::content_capacity(xime_ui::CONTENT_COLUMNS_MAX)
        );
    }
}
