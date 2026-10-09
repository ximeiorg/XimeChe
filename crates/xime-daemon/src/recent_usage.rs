//! 面板「最近使用」记录（表情 / 符号各自一份）。
//!
//! 语义对齐 XimeYao `recent_usage.rs`（同源于安卓 `RecentUsageStore`）：
//! - **LRU，不是频次**：点一次置顶去重、最久没用排末尾、超过上限截断；
//! - **上限 32**：正好是网格页一页容量（8 列 × 4 行），「最近」标签只有一页；
//! - **落在 `~/.config/xime/recent_usage.json`**（安卓 SharedPreferences 等价物），
//!   键名 `recent_emojis` / `recent_symbols` 与安卓一致；
//! - 读写失败只记日志当空表，绝不影响上屏。

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// 最近使用上限（对齐安卓 `RecentUsageStore.MAX_COUNT`，= 网格一页容量）。
pub const MAX_COUNT: usize = 32;

/// 记录种类（JSON 键名与安卓 SharedPreferences key 同名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentKind {
    Emoji,
    Symbol,
}

impl RecentKind {
    fn key(self) -> &'static str {
        match self {
            Self::Emoji => "recent_emojis",
            Self::Symbol => "recent_symbols",
        }
    }
}

/// 测试走 temp 目录（进程级唯一文件），绝不写真实用户文件。
///
/// 2026-10-03 事故：测试直接读写 `~/.config/xime/recent_usage.json`，
/// 把实机用户的「最近使用」覆盖成了测试数据（e8~e39）——用户在表情页
/// 「最近」标签看到一格格的 e20/e39。教训：测试与用户数据的路径必须隔离。
#[cfg(test)]
fn store_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "xime-recent-usage-test-{}.json",
        std::process::id()
    ))
}

#[cfg(not(test))]
fn store_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home)
        .join(".config")
        .join(xime_config::app_metadata().config_dir_name)
        .join("recent_usage.json")
}

fn state_slot() -> &'static Mutex<Option<serde_json::Value>> {
    static SLOT: OnceLock<Mutex<Option<serde_json::Value>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 读内存缓存（未加载时从磁盘读一次）；文件缺失/损坏返回空表。
fn cached() -> serde_json::Value {
    let mut slot = state_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(state) = slot.as_ref() {
        return state.clone();
    }
    let state = std::fs::read_to_string(store_path())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    *slot = Some(state.clone());
    state
}

/// 读某类最近使用（最常用在前）。
pub fn load(kind: RecentKind) -> Vec<String> {
    let state = cached();
    state[kind.key()]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 记录一次使用：置顶去重、截断到 MAX_COUNT，并落盘（写失败只记日志）。
pub fn push(kind: RecentKind, value: &str) {
    if value.is_empty() {
        return;
    }
    let mut slot = state_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut state = slot.clone().unwrap_or_else(|| serde_json::json!({}));
    let list: Vec<String> = state[kind.key()]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let mut list = list;
    list.retain(|v| v != value);
    list.insert(0, value.to_string());
    list.truncate(MAX_COUNT);
    state[kind.key()] = serde_json::json!(list);
    *slot = Some(state.clone());
    let path = store_path();
    // 原子写：半截 JSON 会让下次启动静默丢掉全部最近使用记录。
    if let Err(e) = xime_config::atomic_write(
        &path,
        serde_json::to_string(&state).unwrap_or_default().as_bytes(),
    ) {
        tracing::debug!("recent_usage write failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 独占进程内缓存槽的测试：只跑这一个（缓存是进程级单例）。
    #[test]
    fn push_is_lru_dedup_and_truncates() {
        for i in 0..40 {
            push(RecentKind::Emoji, &format!("e{i}"));
        }
        let list = load(RecentKind::Emoji);
        assert_eq!(list.len(), MAX_COUNT);
        assert_eq!(list[0], "e39", "最新置顶");
        push(RecentKind::Emoji, "e20");
        let list = load(RecentKind::Emoji);
        assert_eq!(list[0], "e20");
        assert_eq!(list.iter().filter(|v| *v == "e20").count(), 1);
        // 符号独立存储。
        push(RecentKind::Symbol, "★");
        assert_eq!(load(RecentKind::Symbol), vec!["★".to_string()]);
        assert_eq!(list.len(), MAX_COUNT, "表情列表不受符号影响");
    }
}
