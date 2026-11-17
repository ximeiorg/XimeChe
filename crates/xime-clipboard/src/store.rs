//! 剪贴板 / 快捷发送存储——**全平台唯一实现**在 libximecore
//! （`xime-config::clipboard_store`，统一库 `xime.db`）。
//!
//! 本模块只做两件事：把 `xime_config::clipboard_store` 的 API 转出去，以及
//! 提供输入法进程内的单例。
//!
//! **为什么不再自己建库**：libximecore 已把剪贴板从独立的 `clipboard.db`
//! 迁进统一库 `xime.db`（整库 schema v7，`clipboard_entries` 仍是 Android
//! v4 列）。设置程序读 `xime.db`、daemon 还写 `clipboard.db` 时，两边的
//! 界面表现是「面板看得见、设置页历史一片空白 / 一直停在旧快照」，而数据库
//! 里其实什么都有——这正是 macOS 版 D21 踩过的坑。存储只留一份，schema 由
//! `xime_config::app_db` 单点升级，两个进程操作同一张表。
//!
//! 平台层留在本 crate：`watcher`（Wayland data-control 监听）和
//! `system_clipboard`（写回，Ctrl+V 用）。

use std::sync::{Arc, OnceLock};

pub use xime_config::clipboard_store::{now_millis, ClipboardEntry, ClipboardStore};

/// 项目全局存储单例（输入法进程内唯一）。
static STORE: OnceLock<Arc<ClipboardStore>> = OnceLock::new();

/// 初始化全局存储（幂等，第二个调用直接返回已有实例）。
///
/// 路径由 libximecore 决定（`xime.db` 与 Rime 用户目录同级），不再由本仓
/// 拼——设置程序用的是同一个函数，两边各算各的就会再次分叉。
pub fn init() -> Arc<ClipboardStore> {
    STORE
        .get_or_init(|| {
            let db_path = xime_config::clipboard_store::default_db_path();
            match ClipboardStore::open(&db_path) {
                Ok(store) => Arc::new(store),
                Err(e) => {
                    eprintln!("[xime-clipboard] open {} failed: {e}", db_path.display());
                    // 打开失败也提供一个（空）实例，避免后续面板操作 panic。
                    Arc::new(
                        ClipboardStore::open_in_memory()
                            .expect("in-memory sqlite should always open"),
                    )
                }
            }
        })
        .clone()
}

/// 取全局存储实例（未调用 [`init`] 时返回 None）。
pub fn store() -> Option<Arc<ClipboardStore>> {
    STORE.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 两个独立连接（模拟 daemon 写 + 设置程序读）必须看到同一份数据。
    ///
    /// 这是本次迁移的核心契约：之前 daemon 写 `clipboard.db`、设置程序读
    /// `xime.db`，两个文件各自都有数据，界面上却像「历史没更新」。
    #[test]
    fn two_handles_share_one_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("xime.db");
        let writer = ClipboardStore::open(&path).unwrap();
        let reader = ClipboardStore::open(&path).unwrap();

        writer.upsert_and_trim("跨进程可见", now_millis()).unwrap();
        let items = reader.list_clipboard(10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "跨进程可见");
    }

    /// 打开默认路径时父目录不存在也要能建出来（首装场景）。
    #[test]
    fn open_creates_missing_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/xime.db");
        let store = ClipboardStore::open(&path).unwrap();
        assert_eq!(store.count_clipboard().unwrap(), 0);
        assert!(path.exists());
    }
}
