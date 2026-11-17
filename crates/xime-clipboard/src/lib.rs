//! 剪贴板 / 快捷发送的**平台层**：Wayland 监听与系统剪贴板写回。
//!
//! - `store`：转出 libximecore 的统一存储（`xime-config::clipboard_store`，
//!   统一库 `xime.db`）——本 crate 不再自己建库，避免与设置程序各写一个文件；
//! - `watcher`：Wayland 剪贴板监听（data-control 协议），替代 macOS 的
//!   `NSPasteboard.changeCount` 轮询；
//! - `system_clipboard`：写回系统剪贴板（远端同步内容让 Ctrl+V 可用）。
//!
//! 拆法与 macOS 版（XimeYi）一致：存储归 libximecore，平台差异留在这里。

pub mod store;
pub mod system_clipboard;
pub mod watcher;
