//! 系统剪贴板监听（Wayland data-control 协议）。
//!
//! 替代 macOS 版的 `NSPasteboard.changeCount` 轮询：后台线程建立独立的
//! Wayland 连接，绑定 data-control 设备，事件驱动地捕获 selection 变化，
//! 读取文本并写入存储（存储内部自动去重/置顶/裁剪）。
//!
//! 协议优先级：`ext-data-control-unstable-v1`（标准暂定协议）→
//! `zwlr-data-control-unstable-v1`（KDE/wlroots 均支持）。
//! GNOME 不支持任一协议，此时监听不可用（记录为已知限制）。

use std::collections::HashMap;
use std::io::Read;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use tracing::{debug, error, warn};
use wayland_client::backend::ObjectId;
use wayland_client::globals::{registry_queue_init, GlobalList, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{event_created_child, Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::ext::data_control::v1::client::ext_data_control_device_v1::{
    self, ExtDataControlDeviceV1,
};
use wayland_protocols::ext::data_control::v1::client::ext_data_control_manager_v1::{
    self, ExtDataControlManagerV1,
};
use wayland_protocols::ext::data_control::v1::client::ext_data_control_offer_v1::{
    self, ExtDataControlOfferV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_device_v1::{
    self, ZwlrDataControlDeviceV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_manager_v1::{
    self, ZwlrDataControlManagerV1,
};
use wayland_protocols_wlr::data_control::v1::client::zwlr_data_control_offer_v1::{
    self, ZwlrDataControlOfferV1,
};

use crate::store::{now_millis, ClipboardStore};

/// UTF-8 文本 MIME（优先）。
const MIME_UTF8: &str = "text/plain;charset=utf-8";
/// 纯文本 MIME（回退）。
const MIME_PLAIN: &str = "text/plain";
/// 单次剪贴板读取上限（1 MiB），防止异常大内容长时间阻塞。
const MAX_READ_BYTES: u64 = 1024 * 1024;
/// 单次剪贴板读取超时：源应用不响应时放弃本次捕获，避免挂死监听线程。
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// 捕获回调（同步桥等消费方）。
pub type CapturedCallback = Box<dyn Fn(&str) + Send + Sync>;

/// 启动剪贴板监听后台线程（进程生命周期内常驻）。
pub fn spawn_watcher(store: Arc<ClipboardStore>) -> JoinHandle<()> {
    spawn_watcher_with_callback(store, None)
}

/// 同上，并在每次捕获文本时回调 `on_captured`（剪贴板同步桥等消费方）。
pub fn spawn_watcher_with_callback(
    store: Arc<ClipboardStore>,
    on_captured: Option<CapturedCallback>,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("xime-clipboard-watcher".into())
        .spawn(move || run_watcher(store, on_captured))
        .expect("spawn clipboard watcher thread")
}

struct WatcherState {
    store: Arc<ClipboardStore>,
    connection: Connection,
    /// 捕获回调（剪贴板同步桥），None = 不通知。
    captured_cb: Option<CapturedCallback>,
    /// 协议对象保活（drop 不销毁协议对象，但保持引用清晰）。
    #[allow(dead_code)]
    ext_manager: Option<ExtDataControlManagerV1>,
    #[allow(dead_code)]
    wlr_manager: Option<ZwlrDataControlManagerV1>,
    #[allow(dead_code)]
    ext_device: Option<ExtDataControlDeviceV1>,
    #[allow(dead_code)]
    wlr_device: Option<ZwlrDataControlDeviceV1>,
    /// 各 offer 已通告的 MIME 类型（按对象 id）。
    ext_mimes: HashMap<ObjectId, Vec<String>>,
    wlr_mimes: HashMap<ObjectId, Vec<String>>,
    /// 上次捕获的文本（相同文本跳过，减少 DB 写入）。
    last_text: Option<String>,
}

fn run_watcher(store: Arc<ClipboardStore>, on_captured: Option<CapturedCallback>) {
    let Ok(connection) = Connection::connect_to_env() else {
        warn!("Clipboard watcher: cannot connect to $WAYLAND_DISPLAY");
        return;
    };
    let (globals, mut queue): (GlobalList, EventQueue<WatcherState>) =
        match registry_queue_init(&connection) {
            Ok(v) => v,
            Err(e) => {
                warn!("Clipboard watcher: registry init failed: {e}");
                return;
            }
        };
    let qh = queue.handle();

    // 绑定 wl_seat（data-control 设备需要）
    let seat: wl_seat::WlSeat = match globals.bind(&qh, 1..=1, ()) {
        Ok(seat) => seat,
        Err(e) => {
            warn!("Clipboard watcher: no wl_seat: {e}");
            return;
        }
    };

    // 绑定 data-control manager（ext 优先）
    let ext_manager = globals
        .bind::<ExtDataControlManagerV1, _, _>(&qh, 1..=1, ())
        .ok();
    let state = if let Some(manager) = ext_manager {
        debug!("Clipboard watcher: using ext-data-control-v1");
        let device = manager.get_data_device(&seat, &qh, ());
        WatcherState {
            store,
            connection,
            captured_cb: on_captured,
            ext_manager: Some(manager),
            wlr_manager: None,
            ext_device: Some(device),
            wlr_device: None,
            ext_mimes: HashMap::new(),
            wlr_mimes: HashMap::new(),
            last_text: None,
        }
    } else if let Ok(manager) = globals.bind::<ZwlrDataControlManagerV1, _, _>(&qh, 1..=1, ()) {
        debug!("Clipboard watcher: using zwlr-data-control-v1");
        let device = manager.get_data_device(&seat, &qh, ());
        WatcherState {
            store,
            connection,
            captured_cb: on_captured,
            ext_manager: None,
            wlr_manager: Some(manager),
            ext_device: None,
            wlr_device: Some(device),
            ext_mimes: HashMap::new(),
            wlr_mimes: HashMap::new(),
            last_text: None,
        }
    } else {
        warn!(
            "Clipboard watcher: no ext/zwlr data-control protocol on this compositor \
             (GNOME unsupported)"
        );
        return;
    };

    debug!("Clipboard watcher started");
    let mut state = state;
    loop {
        if let Err(e) = queue.blocking_dispatch(&mut state) {
            error!("Clipboard watcher: dispatch failed: {e}");
            break;
        }
    }
}

/// 从 MIME 列表中选择文本类型（UTF-8 优先）。
fn pick_text_mime(mimes: &[String]) -> Option<&str> {
    if mimes.iter().any(|m| m == MIME_UTF8) {
        Some(MIME_UTF8)
    } else if mimes.iter().any(|m| m == MIME_PLAIN) {
        Some(MIME_PLAIN)
    } else {
        None
    }
}

/// 读取 offer 中的文本（socketpair 代替管道，读写两端线程内直接收发）。
/// `receive_and_flush` 负责入队 receive 请求并 flush 连接：receive 只是
/// 入队本地缓冲，必须先 flush 到 compositor，源应用才会向 fd 写数据。
fn read_offer_text(receive_and_flush: impl FnOnce(&UnixStream)) -> Option<String> {
    let (rx, tx) = UnixStream::pair().ok()?;
    receive_and_flush(&tx);
    drop(tx); // 关闭写端，读端在数据读完（EOF）后返回
              // 源应用可能迟迟不写数据（或 offer 已失效），读超时防止挂死监听线程。
    rx.set_read_timeout(Some(READ_TIMEOUT)).ok()?;
    let mut buf = String::new();
    // 限制读取量，避免异常大内容长时间占用监听线程。多读 1 字节用于
    // 判定超限：正好读满上限时无法区分"恰好 1MiB"与"被截断"，截断入库
    // 会让用户上屏残缺文本且毫无感知，所以超限整条拒绝。
    let mut limited = rx.take(MAX_READ_BYTES + 1);
    match limited.read_to_string(&mut buf) {
        Ok(n) if n as u64 > MAX_READ_BYTES => {
            debug!("Clipboard capture skipped: content exceeds {MAX_READ_BYTES} bytes");
            None
        }
        Ok(_) if !buf.is_empty() => Some(buf),
        _ => None,
    }
}

/// 捕获文本入库（与上次相同则跳过）。
fn capture(state: &mut WatcherState, text: String) {
    if state.last_text.as_deref() == Some(text.as_str()) {
        return;
    }
    if let Err(e) = state.store.upsert_and_trim(&text, now_millis()) {
        error!("Clipboard watcher: store failed: {e}");
        return;
    }
    debug!("Clipboard captured: {} chars", text.chars().count());
    if let Some(cb) = &state.captured_cb {
        cb(&text);
    }
    state.last_text = Some(text);
}

/// 从 selection offer 中选取文本 MIME 并读取入库（ext/wlr 两后端共用）。
fn capture_selection(
    state: &mut WatcherState,
    mimes: Vec<String>,
    receive_with_mime: impl FnOnce(&str, &UnixStream),
) {
    let Some(mime) = pick_text_mime(&mimes) else {
        return;
    };
    let connection = state.connection.clone();
    // 关键顺序：先入队 receive 并 flush，再阻塞读。若在 flush 前开始读，
    // compositor 收不到 receive 请求，源应用永不写数据 → 监听线程死锁
    // （此前表现为：复制成功但剪贴板历史始终为空）。
    let text = read_offer_text(|fd| {
        receive_with_mime(mime, fd);
        let _ = connection.flush();
    });
    if let Some(text) = text {
        capture(state, text);
    }
}

// ── ext-data-control-unstable-v1 ─────────────────────────────────────────

impl Dispatch<wl_registry::WlRegistry, GlobalListContents, WatcherState> for WatcherState {
    fn event(
        _state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, (), WatcherState> for WatcherState {
    fn event(
        _state: &mut Self,
        _seat: &wl_seat::WlSeat,
        _event: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlManagerV1, (), WatcherState> for WatcherState {
    fn event(
        _state: &mut Self,
        _manager: &ExtDataControlManagerV1,
        _event: ext_data_control_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtDataControlDeviceV1, (), WatcherState> for WatcherState {
    fn event(
        state: &mut Self,
        _device: &ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::DataOffer { id } => {
                state.ext_mimes.insert(id.id(), Vec::new());
            }
            ext_data_control_device_v1::Event::Selection { id: Some(offer) } => {
                let mimes = state.ext_mimes.remove(&offer.id()).unwrap_or_default();
                capture_selection(state, mimes, |mime, fd| {
                    offer.receive(mime.to_string(), fd.as_fd())
                });
                // 读取完成即销毁 offer：proxy drop 不会发 destroy 请求，
                // 不销毁则每次复制在合成器侧泄漏一个 offer 对象。
                offer.destroy();
            }
            ext_data_control_device_v1::Event::Selection { id: None } => {}
            _ => {}
        }
    }

    // data_offer 事件会创建新的 offer 对象：没有这个特化，wayland-client 会在
    // 派发到该事件时 panic（"Missing event_created_child specialization"），
    // 且 panic 发生在不可展开的调用栈里，直接 abort 整个进程（表现为
    // 复制内容后输入法整体崩溃退出）。
    event_created_child!(WatcherState, ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ExtDataControlOfferV1, (), WatcherState> for WatcherState {
    fn event(
        state: &mut Self,
        offer: &ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            state
                .ext_mimes
                .entry(offer.id())
                .or_default()
                .push(mime_type);
        }
    }
}

// ── zwlr-data-control-unstable-v1（回退） ────────────────────────────────

impl Dispatch<ZwlrDataControlManagerV1, (), WatcherState> for WatcherState {
    fn event(
        _state: &mut Self,
        _manager: &ZwlrDataControlManagerV1,
        _event: zwlr_data_control_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrDataControlDeviceV1, (), WatcherState> for WatcherState {
    fn event(
        state: &mut Self,
        _device: &ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::DataOffer { id } => {
                state.wlr_mimes.insert(id.id(), Vec::new());
            }
            zwlr_data_control_device_v1::Event::Selection { id: Some(offer) } => {
                let mimes = state.wlr_mimes.remove(&offer.id()).unwrap_or_default();
                capture_selection(state, mimes, |mime, fd| {
                    offer.receive(mime.to_string(), fd.as_fd())
                });
                // 读取完成即销毁 offer：proxy drop 不会发 destroy 请求，
                // 不销毁则每次复制在合成器侧泄漏一个 offer 对象。
                offer.destroy();
            }
            zwlr_data_control_device_v1::Event::Selection { id: None } => {}
            _ => {}
        }
    }

    event_created_child!(WatcherState, ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlOfferV1, (), WatcherState> for WatcherState {
    fn event(
        state: &mut Self,
        offer: &ZwlrDataControlOfferV1,
        event: zwlr_data_control_offer_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_offer_v1::Event::Offer { mime_type } = event {
            state
                .wlr_mimes
                .entry(offer.id())
                .or_default()
                .push(mime_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_text_mime() {
        assert_eq!(
            pick_text_mime(&["text/plain".to_string()]),
            Some(MIME_PLAIN)
        );
        assert_eq!(
            pick_text_mime(&["text/html".to_string(), MIME_UTF8.to_string()]),
            Some(MIME_UTF8),
            "UTF-8 变体优先"
        );
        assert_eq!(
            pick_text_mime(&[MIME_PLAIN.to_string(), MIME_UTF8.to_string()]),
            Some(MIME_UTF8)
        );
        assert_eq!(pick_text_mime(&["image/png".to_string()]), None);
        assert_eq!(pick_text_mime(&[]), None);
    }
}
