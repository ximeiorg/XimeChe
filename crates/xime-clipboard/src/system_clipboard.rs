//! 系统剪贴板写入（远端同步内容落到本机，Ctrl+V 才能拿到）。
//!
//! 读侧在 [`crate::watcher`]（Wayland data-control 监听）；写侧这里用 arboard：
//! 它在 Linux 上同样走 `wl-clipboard-rs` 的 data-control 后端，且与
//! libximecore 的设置程序、ximed 桌面端保持同一套实现，行为可预期。
//!
//! **为什么不在 watcher 的同一连接上 `set_selection`**：data-control 的
//! 数据源在被粘贴时必须持续响应 `send` 事件，而 watcher 线程阻塞在
//! `blocking_dispatch` 里；要做就得把整个监听循环改成可被 mpsc 唤醒的轮询，
//! 并额外实现 source 事件分发。arboard 已经把这套工作做完（内部自带服务线程），
//! 先复用它，后续若要去掉依赖再置换实现，接口不变。

use std::fmt;

use tracing::{debug, warn};

/// 写入系统剪贴板的能力。
///
/// 抽成 trait 是为了让同步桥的单测能注入内存实现：CI/沙箱里没有
/// Wayland 合成器，`arboard::Clipboard::new()` 会失败。
pub trait ClipboardSink: Send {
    /// 写入文本；返回是否成功。
    fn set_text(&mut self, text: &str) -> bool;
}

impl fmt::Debug for dyn ClipboardSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClipboardSink")
    }
}

/// arboard 实现的系统剪贴板。
///
/// 懒初始化：输入法启动时还没有远端内容，没必要早早建连接；首次写入再试，
/// 失败只记一次日志（没有 Wayland 会话时不刷屏）。
pub struct SystemClipboard {
    inner: Option<arboard::Clipboard>,
    warned: bool,
}

impl SystemClipboard {
    pub fn new() -> Self {
        Self {
            inner: None,
            warned: false,
        }
    }

    fn ensure(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.inner.is_none() {
            match arboard::Clipboard::new() {
                Ok(c) => {
                    debug!("System clipboard: arboard backend ready");
                    self.inner = Some(c);
                }
                Err(e) => {
                    if !self.warned {
                        warn!("System clipboard: init failed: {e}");
                        self.warned = true;
                    }
                    return None;
                }
            }
        }
        self.inner.as_mut()
    }
}

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardSink for SystemClipboard {
    fn set_text(&mut self, text: &str) -> bool {
        let Some(clipboard) = self.ensure() else {
            return false;
        };
        match clipboard.set_text(text.to_string()) {
            Ok(()) => true,
            Err(e) => {
                warn!("System clipboard: set_text failed: {e}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 只有 `SystemClipboard::new` 是无副作用的构造；真正写入需要合成器，
    /// 这里不测（由 daemon 的同步桥用假 sink 覆盖逻辑）。
    #[test]
    fn test_new_does_not_connect() {
        let cb = SystemClipboard::new();
        assert!(cb.inner.is_none());
        assert!(!cb.warned);
    }
}
